//! LanceDB-backed vector store for embeddings.
//!
//! Provides [`LanceStore`]: a thin async wrapper around a local LanceDB
//! dataset that stores `(id, vector, model_id)` triples and supports
//! cosine-similarity nearest-neighbour queries.
//!
//! All public methods are `async` because the `lancedb` Rust SDK is
//! async-on-tokio. The store uses a single fixed-size vector column
//! (`FixedSizeList<Float32>`) — required by LanceDB for vector search.
//! The dimension is inferred lazily from the first [`LanceStore::upsert`]
//! call (which is what creates the underlying `vectors` table), and is
//! then enforced on every subsequent write/query.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, RecordBatchReader,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::DistanceType;
use lancedb::query::{ExecutableQuery, QueryBase, Select};

/// Default chunk size used when materialising the result of an `ann`
/// query into a single `RecordBatch` for id extraction.
const ANN_BATCH_BUFFER: usize = 1024;

/// Name of the single table that backs the store.
pub const DEFAULT_TABLE_NAME: &str = "vectors";

/// LanceDB-backed vector store.
///
/// Stores `(id, vector, model_id)` triples in a local LanceDB dataset and
/// supports cosine-similarity ANN queries over the `vector` column.
///
/// The vector column is a `FixedSizeList<Float32>` of dimension `dim`. `dim`
/// is discovered either by opening an existing table (in [`LanceStore::new`])
/// or by the first call to [`LanceStore::upsert`] (which lazily creates the
/// table with the dimension of the first vector seen).
#[derive(Clone)]
pub struct LanceStore {
    db: lancedb::Connection,
    table_name: String,
    table: Arc<Mutex<Option<lancedb::Table>>>,
    dim: Arc<Mutex<Option<usize>>>,
}

impl std::fmt::Debug for LanceStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanceStore")
            .field("table_name", &self.table_name)
            .field("dim", &*self.dim.lock().expect("dim mutex poisoned"))
            .field(
                "table_open",
                &self.table.lock().expect("table mutex poisoned").is_some(),
            )
            .finish()
    }
}

impl LanceStore {
    /// Open (or create) the LanceDB-backed store at `path`.
    ///
    /// The directory at `path` is created if missing. If a `vectors` table
    /// already exists, its schema is inspected to recover the vector
    /// dimension; otherwise the table is created on the first
    /// [`LanceStore::upsert`] call.
    pub async fn new(path: &Path) -> Result<Self> {
        // Make sure the directory exists — `lancedb::connect` does not
        // create nested directories for us.
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("creating lance dataset dir at {}", path.display()))?;
        }

        let uri = path
            .to_str()
            .ok_or_else(|| anyhow!("lance dataset path is not valid UTF-8: {}", path.display()))?;

        let db = lancedb::connect(uri)
            .execute()
            .await
            .with_context(|| format!("connecting to lance dataset at {}", uri))?;

        let table_name = DEFAULT_TABLE_NAME.to_string();

        // Probe for an existing table so we can recover the stored
        // dimension and cache the `Table` handle.
        let (table, dim) = match db.open_table(&table_name).execute().await {
            Ok(t) => {
                let dim = infer_dim_from_table(&t).await?;
                (Some(t), dim)
            }
            Err(lancedb::Error::TableNotFound { .. }) => (None, None),
            Err(e) => {
                return Err(anyhow!(e))
                    .with_context(|| format!("opening existing table {table_name:?}"));
            }
        };

        Ok(Self {
            db,
            table_name,
            table: Arc::new(Mutex::new(table)),
            dim: Arc::new(Mutex::new(dim)),
        })
    }

    /// Insert-or-replace a single vector by `id`.
    ///
    /// If a row with the same `id` already exists, it is deleted before
    /// the new row is appended, so the table contains exactly one row
    /// per id after the call returns. On the first call (when the
    /// underlying table does not yet exist), the table is created with
    /// `vector` as a `FixedSizeList<Float32>` of dimension `vector.len()`.
    pub async fn upsert(
        &self,
        id: impl Into<String>,
        model_id: impl Into<String>,
        vector: &[f32],
    ) -> Result<()> {
        let id = id.into();
        let model_id = model_id.into();
        if vector.is_empty() {
            return Err(anyhow!("cannot upsert an empty vector for id {id:?}"));
        }

        // Lock the dimension for the whole upsert so we can't race with a
        // concurrent first-upsert that would otherwise create the table
        // with a different size.
        let mut dim_guard = self.dim.lock().expect("dim mutex poisoned");
        let dim = match *dim_guard {
            Some(d) => {
                if d != vector.len() {
                    return Err(anyhow!(
                        "vector dimension mismatch: table dim is {d}, upserted vector has len {}",
                        vector.len()
                    ));
                }
                d
            }
            None => {
                let d = vector.len();
                *dim_guard = Some(d);
                d
            }
        };

        // Make sure the table exists with the right schema.
        let table = self.ensure_table(dim).await?;

        // Replace existing row with this id, if any.
        // SQL string literals need single-quote escaping — `id` is the
        // primary key and may contain arbitrary characters.
        let predicate = format!("id = {}", sql_quote_string(&id));
        // `delete` is a no-op (returns `Ok(DeleteResult{...})`) when no
        // rows match, so we don't need a separate existence check.
        table
            .delete(predicate.as_str())
            .await
            .map_err(|e| anyhow!(e))
            .with_context(|| format!("deleting existing row with id {id:?}"))?;

        // Build a one-row RecordBatch and append it.
        let schema: SchemaRef = Arc::new(table_schema(dim));
        let batch = build_record_batch(&schema, &[id.as_str()], &[model_id.as_str()], &[vector])?;
        append_batch(&table, batch).await?;

        // Refresh cached handle in case the dataset was just created.
        *self.table.lock().expect("table mutex poisoned") = Some(table);

        Ok(())
    }

    /// Return the ids of the `k` rows whose `vector` is closest to
    /// `query` under cosine similarity.
    ///
    /// The returned `Vec<String>` is ordered from most-similar to
    /// least-similar, and contains at most `k` entries (fewer if the
    /// table has fewer than `k` rows).
    pub async fn ann(&self, query: &[f32], k: usize) -> Result<Vec<String>> {
        if k == 0 {
            return Ok(Vec::new());
        }

        let dim = {
            let guard = self.dim.lock().expect("dim mutex poisoned");
            match *guard {
                Some(d) => d,
                None => {
                    return Err(anyhow!(
                        "cannot run ann() before the first upsert: vector dimension unknown"
                    ));
                }
            }
        };

        if query.len() != dim {
            return Err(anyhow!(
                "query dimension mismatch: table dim is {dim}, query has len {}",
                query.len()
            ));
        }

        let table_guard = self.table.lock().expect("table mutex poisoned");
        let table = table_guard
            .as_ref()
            .ok_or_else(|| anyhow!("vectors table has not been initialised yet"))?;

        // We only need the `id` column for the result; selecting only
        // the column we need avoids loading vectors off disk.
        let stream = table
            .query()
            .select(Select::Columns(vec!["id".to_string()]))
            .nearest_to(query)
            .map_err(|e| anyhow!(e).context("building nearest_to query"))?
            .distance_type(DistanceType::Cosine)
            .limit(k)
            .execute()
            .await
            .map_err(|e| anyhow!(e).context("executing nearest_to query"))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| anyhow!(e).context("collecting ann result batches"))?;

        drop(table_guard);

        let mut ids: Vec<String> = Vec::with_capacity(ANN_BATCH_BUFFER);
        for batch in &batches {
            let col = batch
                .column_by_name("id")
                .ok_or_else(|| anyhow!("ann result batch missing `id` column"))?;
            let arr = col
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("`id` column is not a StringArray"))?;
            for i in 0..arr.len() {
                if arr.is_valid(i) {
                    ids.push(arr.value(i).to_string());
                }
            }
            if ids.len() >= k {
                break;
            }
        }
        ids.truncate(k);
        Ok(ids)
    }

    /// Return the dimension of stored vectors, if known.
    pub fn dim(&self) -> Option<usize> {
        *self.dim.lock().expect("dim mutex poisoned")
    }

    /// Lazily create the underlying `vectors` table with the given dim,
    /// caching the resulting `Table` handle on `self`.
    async fn ensure_table(&self, dim: usize) -> Result<lancedb::Table> {
        {
            let guard = self.table.lock().expect("table mutex poisoned");
            if let Some(t) = guard.as_ref() {
                return Ok(t.clone());
            }
        }

        let schema: SchemaRef = Arc::new(table_schema(dim));
        let table = self
            .db
            .create_empty_table(&self.table_name, schema)
            .execute()
            .await
            .map_err(|e| anyhow!(e))
            .with_context(|| format!("creating vectors table with dim {dim}"))?;

        *self.table.lock().expect("table mutex poisoned") = Some(table.clone());
        Ok(table)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the Arrow schema for the `vectors` table.
fn table_schema(dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
        Field::new("model_id", DataType::Utf8, false),
    ])
}

/// Build a one-row `RecordBatch` from parallel slices of ids, model_ids,
/// and vectors. All three slices must have the same length (1).
fn build_record_batch(
    schema: &SchemaRef,
    ids: &[&str],
    model_ids: &[&str],
    vectors: &[&[f32]],
) -> Result<RecordBatch> {
    debug_assert_eq!(ids.len(), model_ids.len(), "ids/model_ids length mismatch");
    debug_assert_eq!(ids.len(), vectors.len(), "ids/vectors length mismatch");

    let id_array = StringArray::from(ids.to_vec());
    let model_array = StringArray::from(model_ids.to_vec());

    let flat_values: Vec<f32> = vectors.iter().flat_map(|v| v.iter().copied()).collect();
    let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
    let values_array = Float32Array::from(flat_values);

    // `nulls` is unused here (we never produce null vector entries), but
    // `FixedSizeListArray::try_new` requires the inner array to be
    // length-compatible.
    let vector_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim as i32,
        Arc::new(values_array),
        None,
    )
    .map_err(|e| anyhow!(e).context("building FixedSizeListArray for `vector` column"))?;

    RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(id_array),
            Arc::new(vector_array),
            Arc::new(model_array),
        ],
    )
    .map_err(|e| anyhow!(e).context("building RecordBatch"))
}

/// Append a single `RecordBatch` to `table` via the `AddDataBuilder`.
async fn append_batch(table: &lancedb::Table, batch: RecordBatch) -> Result<()> {
    // `lancedb::Table::add` requires a `Scannable`. In LanceDB 0.31 the
    // supported Arrow streaming adapter is a boxed `RecordBatchReader`, not
    // the concrete `RecordBatchIterator` itself.
    let schema = batch.schema();
    let batches = vec![Ok(batch)];
    let iter = RecordBatchIterator::new(batches.into_iter(), schema);
    let reader: Box<dyn RecordBatchReader + Send> = Box::new(iter);

    table
        .add(reader)
        .execute()
        .await
        .map_err(|e| anyhow!(e).context("appending record batch to vectors table"))?;
    Ok(())
}

/// Inspect the schema of an already-open table to recover the vector
/// dimension. Returns `Ok(None)` if the table exists but has no `vector`
/// column (or the column is not a `FixedSizeList<Float32>`).
async fn infer_dim_from_table(table: &lancedb::Table) -> Result<Option<usize>> {
    let schema = table
        .schema()
        .await
        .map_err(|e| anyhow!(e).context("reading schema of existing vectors table"))?;
    let Some(field) = schema.field_with_name("vector").ok() else {
        return Ok(None);
    };
    match field.data_type() {
        DataType::FixedSizeList(_, n) if *n > 0 => Ok(Some(*n as usize)),
        _ => Err(anyhow!(
            "existing `vectors` table has a `vector` column of unexpected type {}",
            field.data_type()
        )),
    }
}

/// Escape a string for inclusion as a SQL string literal in a
/// `Table::delete` predicate. The simplest correct approach is to double
/// any embedded single-quotes and wrap the whole thing in single quotes.
fn sql_quote_string(s: &str) -> String {
    let escaped = s.replace('\'', "''");
    format!("'{escaped}'")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a deterministic 4-dim unit-ish vector for the given key.
    fn vec_for(seed: f32) -> Vec<f32> {
        // Use simple basis-style vectors so cosine similarities are
        // easy to eyeball in tests.
        match seed as i32 {
            0 => vec![1.0, 0.0, 0.0, 0.0],
            1 => vec![0.0, 1.0, 0.0, 0.0],
            2 => vec![0.0, 0.0, 1.0, 0.0],
            3 => vec![0.0, 0.0, 0.0, 1.0],
            4 => vec![1.0, 1.0, 0.0, 0.0],
            5 => vec![1.0, 1.0, 1.0, 0.0],
            _ => vec![1.0, 1.0, 1.0, 1.0],
        }
    }

    #[tokio::test]
    async fn new_creates_dataset_dir_and_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lance-data");

        let store = LanceStore::new(&path).await.unwrap();
        assert!(
            path.exists(),
            "LanceStore::new should create the dataset dir"
        );
        assert!(store.dim().is_none(), "dim is unknown before any upsert");

        store.upsert("a", "m1", &vec_for(0.0)).await.unwrap();
        assert_eq!(store.dim(), Some(4));

        store.upsert("b", "m1", &vec_for(1.0)).await.unwrap();
        store.upsert("c", "m1", &vec_for(2.0)).await.unwrap();

        // Reopen and confirm the data is durable.
        drop(store);
        let store2 = LanceStore::new(&path).await.unwrap();
        assert_eq!(store2.dim(), Some(4), "dim should be recovered from disk");
        let hits = store2.ann(&vec_for(0.0), 3).await.unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0], "a", "exact match should rank first under cosine");
    }

    #[tokio::test]
    async fn append_batch_persists_a_single_record_batch() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path()).await.unwrap();
        let table = store.ensure_table(4).await.unwrap();
        let schema: SchemaRef = Arc::new(table_schema(4));
        let vector = vec_for(0.0);
        let batch = build_record_batch(&schema, &["a"], &["m1"], &[&vector]).unwrap();

        append_batch(&table, batch).await.unwrap();

        assert_eq!(table.count_rows(None).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn upsert_replaces_existing_id() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path()).await.unwrap();

        store.upsert("a", "m1", &vec_for(0.0)).await.unwrap();
        store.upsert("b", "m1", &vec_for(0.0)).await.unwrap();
        store.upsert("a", "m2", &vec_for(2.0)).await.unwrap();

        // `a` should now match the second vector, not the first.
        let hits = store.ann(&vec_for(2.0), 1).await.unwrap();
        assert_eq!(hits, vec!["a".to_string()]);

        let hits_first = store.ann(&vec_for(0.0), 1).await.unwrap();
        assert_eq!(
            hits_first,
            vec!["b".to_string()],
            "after replace, `b` should be the nearest to vec_for(0.0)"
        );
    }

    #[tokio::test]
    async fn upsert_rejects_dimension_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path()).await.unwrap();

        store.upsert("a", "m1", &vec_for(0.0)).await.unwrap(); // dim 4
        let err = store.upsert("b", "m1", &[0.0, 0.0]).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("dimension mismatch"),
            "expected dim-mismatch error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn ann_rejects_wrong_dim_query() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path()).await.unwrap();
        store.upsert("a", "m1", &vec_for(0.0)).await.unwrap();

        let err = store.ann(&[0.0, 0.0], 1).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("dimension mismatch"),
            "expected dim-mismatch error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn ann_with_k_zero_returns_empty() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path()).await.unwrap();
        store.upsert("a", "m1", &vec_for(0.0)).await.unwrap();

        let hits = store.ann(&vec_for(0.0), 0).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn sql_quote_string_escapes_single_quotes() {
        assert_eq!(sql_quote_string("abc"), "'abc'");
        assert_eq!(sql_quote_string("a'b"), "'a''b'");
        assert_eq!(sql_quote_string(""), "''");
    }
}

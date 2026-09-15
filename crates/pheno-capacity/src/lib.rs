//! Capacity math spike (H14+ scaffold, L5-117).
//!
//! Absorbed from `KooshaPari/pheno-capacity` v0.2.0 (2026-06-19) per
//! the collection-repo merge plan (L5-117). Source: ~2,200 LOC,
//! `no_std` compatible, zero dependencies, deterministic. 60 unit
//! tests + 6 doc tests.
//!
//! See [`pheno_capacity`] for the actual API.

#![allow(unused_imports)]
#![cfg_attr(not(feature = "alloc"), no_std)]

pub mod pheno_capacity;

pub use pheno_capacity::*;

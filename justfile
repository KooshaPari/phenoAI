# Phenotype-org shared justfile. Imported from phenotype-tooling/just/phenotype.just.
# To override a recipe locally, redefine it after the import.
import? "/Users/kooshapari/CodeProjects/Phenotype/repos/phenotype-tooling/just/phenotype.just"

# Lint with clippy (warnings as errors) AND fmt-check
lint: fmt-check (just --justfile {{justfile_path()}} lint)

# Audit: cargo-deny + cargo-audit (combined)
audit: (just --justfile {{justfile_path()}} deny) (just --justfile {{justfile_path()}} --justfile {{justfile_path()}} audit)
# Grade targets (strictest checks — no caching)
grade:
    @echo "=== Running full grade ==="
    ./grade.sh

grade-fast:
    @echo "=== Running fast grade ==="
    ./grade.sh --fast

grade-json:
    @echo "=== Running grade (JSON) ==="
    ./grade.sh --json

grade-html:
    @echo "=== Running grade (HTML) ==="
    ./grade.sh --html


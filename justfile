# Phenotype-org shared justfile. Imported from phenotype-tooling/just/phenotype.just.
# To override a recipe locally, redefine it after the import.
import? "/Users/kooshapari/CodeProjects/Phenotype/repos/phenotype-tooling/just/phenotype.just"

# Lint with clippy (warnings as errors) AND fmt-check
lint: fmt-check (just --justfile {{justfile_path()}} lint)

# Audit: cargo-deny + cargo-audit (combined)
audit: (just --justfile {{justfile_path()}} deny) (just --justfile {{justfile_path()}} --justfile {{justfile_path()}} audit)

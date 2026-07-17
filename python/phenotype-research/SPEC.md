# Phenotype Research Engine Specification

> Research Engine

**Version**: 1.0 | **Status**: **DEPRECATED** (2026-06-20) | **Last Updated**: 2026-06-20

> **DEPRECATION NOTICE:** This specification is for a deprecated package.
> See [DEPRECATED.md](./DEPRECATED.md) for migration to the successor
> module at `packages/phenotype-research/`.

## Overview

Research Engine for the Phenotype ecosystem.

**Language**: Python

**Key Features**:
- Web scraping, data extraction

## Architecture

```
phenotype-research-engine/
├── src/           # Implementation
├── tests/         # Unit tests
├── docs/          # Documentation
└── examples/      # Usage examples
```

## Quick Start

```bash
# Install
cargo add phenotype-research-engine  # or npm/pip equivalent

# Usage
see examples/ directory
```

## API Reference

See source code documentation.

## Performance Targets

| Metric | Target |
|--------|--------|
| Init time | < 10ms |
| Memory | < 10MB |
| Throughput | 10K ops/sec |

## License

MIT

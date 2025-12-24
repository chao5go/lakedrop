# LakeDrop

[中文说明](README.zh.md)
[Changelog](CHANGELOG.md)

<div align="center">

[![Stars](https://img.shields.io/github/stars/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/stargazers)
[![Forks](https://img.shields.io/github/forks/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/network/members)
[![License](https://img.shields.io/github/license/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/blob/main/LICENSE)

</div>

LakeDrop is a fast, local-first desktop data explorer for large files. Open Parquet/CSV/JSONL/Arrow/Excel, run SQL, and inspect results instantly.

## Features
- Drag-and-drop or open-file workflow
- SQL editor with Cmd/Ctrl+Enter execution
- Virtualized results table with sorting and column resizing
- Schema preview, file metadata, and row count
- CSV/XLSX export for query results
- i18n (EN/中文) and light/dark themes
- Built-in sample datasets for each format

## Supported Formats
- Parquet: `.parquet`, `.parq`
- CSV/TSV: `.csv`, `.tsv`, `.txt` (also `.gz`)
- JSON Lines: `.jsonl`, `.ndjson` (also `.gz`)
- JSON (array): `.json`
- Arrow/IPC: `.arrow`, `.feather`, `.ipc`
- Excel: `.xlsx`, `.xls` (first sheet by default; sheet switcher available)

## Quick Start

Install dependencies:
```bash
pnpm install
```

Run the desktop app:
```bash
pnpm tauri dev
```

Run the frontend only:
```bash
pnpm dev
```

## Sample Data

Built-in samples are bundled and also available in development:
- `public/samples/`
- `src-tauri/resources/samples/`

Regenerate samples:
```bash
cargo run --bin build_samples --manifest-path src-tauri/Cargo.toml
```

## SQL Examples
```sql
SELECT * FROM source LIMIT 10;

SELECT COUNT(*) AS total FROM source;

SELECT group, COUNT(*) AS cnt, AVG(score) AS avg_score
FROM source
GROUP BY group
ORDER BY cnt DESC;
```

## Project Structure
- `src/`: React UI, i18n, styling
- `src-tauri/`: Rust backend, Tauri config, sample generator

## Notes
- JSON support expects either JSON Lines (`.jsonl/.ndjson`) or a JSON array (`.json`).
- Gzip support is implemented for text formats.

## License
MIT or Apache-2.0 (pick one before publishing).

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use calamine::{open_workbook_auto, Data, Reader};
use flate2::read::GzDecoder;
use polars::lazy::dsl::col;
use polars::prelude::*;
use polars::sql::SQLContext;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
struct AppState {
    source: Option<LazyFrame>,
    file_path: Option<PathBuf>,
    file_kind: Option<FileKind>,
    sheets: Vec<String>,
    active_sheet: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Parquet,
    Csv,
    JsonLines,
    Json,
    Arrow,
    Excel,
}

struct FileSpec {
    kind: FileKind,
    compressed: bool,
    extension: String,
}

#[derive(Serialize)]
struct FieldInfo {
    name: String,
    dtype: String,
}

#[derive(Serialize)]
struct FileMetadataResponse {
    file_name: String,
    file_path: String,
    file_size: u64,
    row_count: u64,
    schema: Vec<FieldInfo>,
    sheets: Vec<String>,
    active_sheet: Option<String>,
}

#[derive(Serialize)]
struct ColumnInfo {
    name: String,
    dtype: String,
}

#[derive(Serialize)]
struct QueryResult {
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<serde_json::Value>>,
    row_count: usize,
}

fn gzip_magic(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|err| err.to_string())?;
    let mut buf = [0u8; 2];
    let read = file.read(&mut buf).map_err(|err| err.to_string())?;
    Ok(read == 2 && buf == [0x1f, 0x8b])
}

fn detect_file_kind(path: &Path) -> Result<FileSpec, String> {
    let mut compressed = false;
    let mut ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "gz" {
        compressed = true;
        ext = path
            .file_stem()
            .and_then(|value| Path::new(value).extension())
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
    }

    let kind = match ext.as_str() {
        "parquet" | "parq" => FileKind::Parquet,
        "csv" | "tsv" | "txt" => FileKind::Csv,
        "jsonl" | "ndjson" => FileKind::JsonLines,
        "json" => FileKind::Json,
        "arrow" | "feather" | "ipc" => FileKind::Arrow,
        "xlsx" | "xls" => FileKind::Excel,
        _ => return Err(format!("Unsupported file type: .{ext}")),
    };

    if !compressed {
        compressed = gzip_magic(path).unwrap_or(false);
    }

    Ok(FileSpec {
        kind,
        compressed,
        extension: ext,
    })
}

fn schema_to_fields(schema: &Schema) -> Vec<FieldInfo> {
    schema
        .iter()
        .map(|(name, dtype)| FieldInfo {
            name: name.to_string(),
            dtype: format!("{dtype:?}"),
        })
        .collect()
}

fn excel_cell_to_string(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty => None,
        Data::String(value) => Some(value.to_string()),
        Data::Float(value) => Some(value.to_string()),
        Data::Int(value) => Some(value.to_string()),
        Data::Bool(value) => Some(value.to_string()),
        Data::DateTime(value) => Some(value.to_string()),
        Data::DateTimeIso(value) => Some(value.to_string()),
        Data::DurationIso(value) => Some(value.to_string()),
        Data::Error(value) => Some(format!("{value:?}")),
    }
}

fn load_excel_sheet(
    path: &Path,
    sheet_name: Option<String>,
) -> Result<(DataFrame, Vec<String>, String), String> {
    let mut workbook = open_workbook_auto(path).map_err(|err| err.to_string())?;
    let sheets = workbook.sheet_names().to_vec();
    let active = sheet_name
        .or_else(|| sheets.first().cloned())
        .ok_or("No sheets found in workbook")?;
    let range = workbook
        .worksheet_range(&active)
        .map_err(|err| err.to_string())?;
    let mut rows = range.rows();
    let header_row = rows.next();
    let mut headers = header_row
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(idx, cell)| {
                    excel_cell_to_string(cell)
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| format!("col_{}", idx + 1))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut columns: Vec<Vec<Option<String>>> = vec![Vec::new(); headers.len()];
    for row in rows {
        if row.len() > headers.len() {
            let start = headers.len();
            headers.extend((start..row.len()).map(|idx| format!("col_{}", idx + 1)));
            columns.extend((start..row.len()).map(|_| Vec::new()));
        }
        for (idx, col) in columns.iter_mut().enumerate() {
            let value = row.get(idx).and_then(excel_cell_to_string);
            col.push(value);
        }
    }

    let series = headers
        .iter()
        .zip(columns)
        .map(|(name, values)| Series::new(name, values))
        .collect::<Vec<_>>();

    let df = DataFrame::new(series).map_err(|err| err.to_string())?;
    Ok((df, sheets, active))
}

fn load_lazy_frame(path: &Path, spec: &FileSpec) -> Result<LazyFrame, String> {
    match (spec.kind, spec.compressed) {
        (FileKind::Parquet, false) => LazyFrame::scan_parquet(path, ScanArgsParquet::default())
            .map_err(|err| err.to_string()),
        (FileKind::Csv, false) => LazyCsvReader::new(path)
            .with_separator(if spec.extension == "tsv" { b'\t' } else { b',' })
            .with_try_parse_dates(true)
            .finish()
            .map_err(|err| err.to_string()),
        (FileKind::JsonLines, false) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let df = JsonLineReader::new(file)
                .finish()
                .map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (FileKind::Json, false) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let df = JsonReader::new(file)
                .with_json_format(JsonFormat::Json)
                .finish()
                .map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (FileKind::Arrow, false) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let df = IpcReader::new(file)
                .finish()
                .map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (FileKind::Excel, false) => {
            let (df, _, _) = load_excel_sheet(path, None)?;
            Ok(df.lazy())
        }
        (FileKind::Csv, true) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let mut decoder = GzDecoder::new(file);
            let mut buffer = Vec::new();
            decoder
                .read_to_end(&mut buffer)
                .map_err(|err| err.to_string())?;
            let cursor = Cursor::new(buffer);
            let reader = CsvReadOptions::default()
                .map_parse_options(|options: CsvParseOptions| {
                    options
                        .with_separator(if spec.extension == "tsv" { b'\t' } else { b',' })
                        .with_try_parse_dates(true)
                })
                .into_reader_with_file_handle(cursor);
            let df = reader.finish().map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (FileKind::JsonLines, true) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let mut decoder = GzDecoder::new(file);
            let mut buffer = Vec::new();
            decoder
                .read_to_end(&mut buffer)
                .map_err(|err| err.to_string())?;
            let cursor = Cursor::new(buffer);
            let df = JsonLineReader::new(cursor)
                .finish()
                .map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (FileKind::Json, true) => {
            let file = File::open(path).map_err(|err| err.to_string())?;
            let mut decoder = GzDecoder::new(file);
            let mut buffer = Vec::new();
            decoder
                .read_to_end(&mut buffer)
                .map_err(|err| err.to_string())?;
            let cursor = Cursor::new(buffer);
            let df = JsonReader::new(cursor)
                .with_json_format(JsonFormat::Json)
                .finish()
                .map_err(|err| err.to_string())?;
            Ok(df.lazy())
        }
        (_, true) => Err("Compressed file is not supported for this format".to_string()),
    }
}

fn lazy_row_count(lf: &LazyFrame) -> Result<u64, String> {
    let df = lf
        .clone()
        .select([col("*").len()])
        .collect()
        .map_err(|err| err.to_string())?;
    let series = df.get_columns().get(0).ok_or("Missing count")?;
    let value = series.get(0).map_err(|err| err.to_string())?;
    let count = match value {
        AnyValue::UInt64(value) => value,
        AnyValue::UInt32(value) => value as u64,
        AnyValue::Int64(value) => value as u64,
        AnyValue::Int32(value) => value as u64,
        _ => 0,
    };
    Ok(count)
}

fn any_value_to_json(value: AnyValue) -> serde_json::Value {
    match value {
        AnyValue::Null => serde_json::Value::Null,
        AnyValue::Boolean(value) => serde_json::Value::Bool(value),
        AnyValue::Int8(value) => serde_json::Value::from(value),
        AnyValue::Int16(value) => serde_json::Value::from(value),
        AnyValue::Int32(value) => serde_json::Value::from(value),
        AnyValue::Int64(value) => serde_json::Value::from(value),
        AnyValue::UInt8(value) => serde_json::Value::from(value),
        AnyValue::UInt16(value) => serde_json::Value::from(value),
        AnyValue::UInt32(value) => serde_json::Value::from(value),
        AnyValue::UInt64(value) => serde_json::Value::from(value),
        AnyValue::Float32(value) => serde_json::Value::from(value as f64),
        AnyValue::Float64(value) => serde_json::Value::from(value),
        AnyValue::String(value) => serde_json::Value::String(value.to_string()),
        AnyValue::StringOwned(value) => serde_json::Value::String(value.to_string()),
        AnyValue::Binary(value) => serde_json::Value::String(format!("{value:?}")),
        AnyValue::BinaryOwned(value) => serde_json::Value::String(format!("{value:?}")),
        AnyValue::Date(value) => serde_json::Value::String(value.to_string()),
        AnyValue::Datetime(value, _, _) => serde_json::Value::String(value.to_string()),
        AnyValue::Time(value) => serde_json::Value::String(value.to_string()),
        AnyValue::Duration(value, _) => serde_json::Value::String(value.to_string()),
        AnyValue::List(_) => serde_json::Value::String(value.to_string()),
        _ => serde_json::Value::String(value.to_string()),
    }
}

#[tauri::command]
fn scan_file_metadata(
    path: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileMetadataResponse, String> {
    let path = PathBuf::from(path);
    let spec = detect_file_kind(&path)?;
    let file_size = std::fs::metadata(&path)
        .map(|meta| meta.len())
        .unwrap_or(0);

    let (lf, sheets, active_sheet, row_count, schema) = if spec.kind == FileKind::Excel {
        let (df, sheets, active_sheet) = load_excel_sheet(&path, None)?;
        let schema = df.schema();
        let row_count = df.height() as u64;
        (df.lazy(), sheets, Some(active_sheet), row_count, schema)
    } else {
        let mut lf = load_lazy_frame(&path, &spec)?;
        let schema = lf
            .schema()
            .map_err(|err| err.to_string())?
            .as_ref()
            .clone();
        let row_count = lazy_row_count(&lf).unwrap_or(0);
        (lf, Vec::new(), None, row_count, schema)
    };

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("data")
        .to_string();

    let response = FileMetadataResponse {
        file_name,
        file_path: path.display().to_string(),
        file_size,
        row_count,
        schema: schema_to_fields(&schema),
        sheets,
        active_sheet,
    };

    let mut guard = state.lock().map_err(|_| "State lock failed")?;
    guard.source = Some(lf);
    guard.file_path = Some(path);
    guard.file_kind = Some(spec.kind);
    guard.sheets = response.sheets.clone();
    guard.active_sheet = response.active_sheet.clone();

    Ok(response)
}

#[tauri::command]
fn select_excel_sheet(
    sheet: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileMetadataResponse, String> {
    let mut guard = state.lock().map_err(|_| "State lock failed")?;
    let path = guard
        .file_path
        .as_ref()
        .ok_or("No file loaded. Drag a file to begin.")?;
    if guard.file_kind != Some(FileKind::Excel) {
        return Err("Current file is not an Excel workbook.".to_string());
    }

    let (df, sheets, active_sheet) = load_excel_sheet(path, Some(sheet))?;
    let schema = df.schema();
    let row_count = df.height() as u64;
    let response = FileMetadataResponse {
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("data")
            .to_string(),
        file_path: path.display().to_string(),
        file_size: std::fs::metadata(path)
            .map(|meta| meta.len())
            .unwrap_or(0),
        row_count,
        schema: schema_to_fields(&schema),
        sheets,
        active_sheet: Some(active_sheet),
    };

    guard.source = Some(df.lazy());
    guard.sheets = response.sheets.clone();
    guard.active_sheet = response.active_sheet.clone();

    Ok(response)
}

#[tauri::command]
fn exec_sql(
    sql: String,
    max_rows: Option<usize>,
    state: State<'_, Mutex<AppState>>,
) -> Result<QueryResult, String> {
    let guard = state.lock().map_err(|_| "State lock failed")?;
    let source = guard
        .source
        .as_ref()
        .ok_or("No file loaded. Drag a file to begin.")?;

    let mut ctx = SQLContext::new();
    ctx.register("source", source.clone());
    let df = ctx
        .execute(&sql)
        .map_err(|err| err.to_string())?
        .collect()
        .map_err(|err| err.to_string())?;

    let df = if let Some(max_rows) = max_rows {
        df.head(Some(max_rows))
    } else {
        df
    };

    let columns = df
        .schema()
        .iter_fields()
        .map(|field| ColumnInfo {
            name: field.name().to_string(),
            dtype: format!("{:?}", field.data_type()),
        })
        .collect::<Vec<_>>();

    let row_count = df.height();
    let mut rows = Vec::with_capacity(row_count);
    for row_idx in 0..row_count {
        let mut row = Vec::with_capacity(df.width());
        for series in df.get_columns() {
            let value = series.get(row_idx).map_err(|err| err.to_string())?;
            row.push(any_value_to_json(value));
        }
        rows.push(row);
    }

    Ok(QueryResult {
        columns,
        rows,
        row_count,
    })
}

#[tauri::command]
fn export_query(
    sql: String,
    path: String,
    format: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let guard = state.lock().map_err(|_| "State lock failed")?;
    let source = guard
        .source
        .as_ref()
        .ok_or("No file loaded. Drag a file to begin.")?;
    let mut ctx = SQLContext::new();
    ctx.register("source", source.clone());
    let df = ctx
        .execute(&sql)
        .map_err(|err| err.to_string())?
        .collect()
        .map_err(|err| err.to_string())?;

    let path = PathBuf::from(path);
    match format.as_str() {
        "csv" => {
            let mut file = File::create(path).map_err(|err| err.to_string())?;
            let mut df = df;
            CsvWriter::new(&mut file)
                .finish(&mut df)
                .map_err(|err| err.to_string())?;
        }
        "xlsx" => {
            let mut book = umya_spreadsheet::new_file();
            let sheet = book
                .get_sheet_by_name_mut("Sheet1")
                .ok_or("Missing sheet")?;
            for (col_idx, name) in df.get_column_names().iter().enumerate() {
                sheet
                    .get_cell_mut(((col_idx + 1) as u32, 1u32))
                    .set_value(name.to_string());
            }
            for row_idx in 0..df.height() {
                for (col_idx, series) in df.get_columns().iter().enumerate() {
                    let value = series.get(row_idx).map_err(|err| err.to_string())?;
                    sheet
                        .get_cell_mut(((col_idx + 1) as u32, (row_idx + 2) as u32))
                        .set_value(value.to_string());
                }
            }
            umya_spreadsheet::writer::xlsx::write(&book, path)
                .map_err(|err| err.to_string())?;
        }
        _ => return Err("Unsupported export format".to_string()),
    }

    Ok(())
}

#[tauri::command]
fn resolve_sample_path(file_name: String, app: AppHandle) -> Result<String, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err: tauri::Error| err.to_string())?;
    let resource_path = resource_dir.join("samples").join(&file_name);
    if resource_path.exists() {
        return Ok(resource_path.display().to_string());
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = PathBuf::from(manifest_dir)
            .join("..")
            .join("public")
            .join("samples")
            .join(&file_name);
        if dev_path.exists() {
            return Ok(dev_path.display().to_string());
        }
    }

    Err("Sample file not found".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState::default()))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_file_metadata,
            select_excel_sheet,
            resolve_sample_path,
            exec_sql,
            export_query
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

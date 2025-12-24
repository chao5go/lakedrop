use std::fs;
use std::path::PathBuf;

use polars::prelude::*;

fn sample_df() -> PolarsResult<DataFrame> {
    df!(
        "id" => &[1i64, 2, 3, 4, 5],
        "name" => &["alpha", "bravo", "charlie", "delta", "echo"],
        "score" => &[98.5f64, 76.2, 88.0, 91.4, 69.0],
        "active" => &[true, false, true, true, false],
        "group" => &["A", "B", "A", "B", "C"]
    )
}

fn write_csv(df: &DataFrame, path: &PathBuf) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    let mut df = df.clone();
    CsvWriter::new(&mut file).finish(&mut df)?;
    Ok(())
}

fn write_jsonl(df: &DataFrame, path: &PathBuf) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    let mut df = df.clone();
    JsonWriter::new(&mut file)
        .with_json_format(JsonFormat::JsonLines)
        .finish(&mut df)?;
    Ok(())
}

fn write_parquet(df: &DataFrame, path: &PathBuf) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    let mut df = df.clone();
    ParquetWriter::new(&mut file).finish(&mut df)?;
    Ok(())
}

fn write_arrow(df: &DataFrame, path: &PathBuf) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    let mut df = df.clone();
    IpcWriter::new(&mut file).finish(&mut df)?;
    Ok(())
}

fn write_xlsx(df: &DataFrame, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
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
            let value = series.get(row_idx)?;
            sheet
                .get_cell_mut(((col_idx + 1) as u32, (row_idx + 2) as u32))
                .set_value(value.to_string());
        }
    }

    umya_spreadsheet::writer::xlsx::write(&book, path)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let public_dir = manifest_dir.join("..").join("public").join("samples");
    let resources_dir = manifest_dir.join("resources").join("samples");

    fs::create_dir_all(&public_dir)?;
    fs::create_dir_all(&resources_dir)?;

    let df = sample_df()?;

    for dir in [&public_dir, &resources_dir] {
        write_csv(&df, &dir.join("sample.csv"))?;
        write_jsonl(&df, &dir.join("sample.jsonl"))?;
        write_parquet(&df, &dir.join("sample.parquet"))?;
        write_arrow(&df, &dir.join("sample.arrow"))?;
        write_xlsx(&df, &dir.join("sample.xlsx"))?;
    }

    println!(
        "Samples written to {} and {}",
        public_dir.display(),
        resources_dir.display()
    );
    Ok(())
}

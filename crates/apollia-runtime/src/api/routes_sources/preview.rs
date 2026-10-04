//! `POST /api/v1/sources/preview`: what a batch on a workbook or a CSV file
//! would read, before it is launched.
//!
//! The file is read where the runtime runs, on a blocking thread; nothing is
//! written and nothing is kept. The answer proposes a sheet and a header row,
//! shows the first rows, and follows a sheet or a header row the caller picks.

use std::path::PathBuf;

use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_sources::{
    preview_table, Column, PreviewError, PreviewRow, ReadError, SheetSummary, TablePreview,
};

use crate::api::routes_llm::servers::{api_err, ApiError};

/// Body of `POST /api/v1/sources/preview`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSourceBody {
    /// Absolute path of a `.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods` or `.csv` file.
    pub path: String,
    /// The sheet to show; the proposed sheet when absent.
    #[serde(default)]
    pub sheet: Option<String>,
    /// The header row, from 1, on the shown sheet; the proposed row when absent.
    #[serde(default)]
    pub header_row: Option<u32>,
}

/// One sheet of the file.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSheet {
    /// Name of the sheet.
    pub name: String,
    /// How many of its rows hold at least one non-blank cell.
    pub non_empty_rows: u64,
}

/// One column of the table under the header row.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewColumn {
    /// Column letter, as the spreadsheet shows it; a batch checks a column by it.
    pub letter: String,
    /// The name its header cell gives it, made unique.
    pub name: String,
}

/// One row of the preview.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRowView {
    /// Row number, from 1.
    pub row: u32,
    /// The text of each cell, column A first.
    pub cells: Vec<String>,
}

/// What a batch on the file would read.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourcePreviewResponse {
    /// `xlsx`, `xlsm`, `xlsb`, `xls`, `ods` or `csv`.
    pub format: String,
    /// The batch source type that reads the file: `spreadsheet` or `csv`.
    pub source_type: String,
    /// Every sheet, in the file's order; a CSV file has one, named after it.
    pub sheets: Vec<PreviewSheet>,
    /// The sheet proposed: the only one, or the one with the most non-empty rows.
    pub proposed_sheet: Option<String>,
    /// The sheet shown: the one asked for, or the proposed one.
    pub sheet: Option<String>,
    /// The header row proposed on the shown sheet.
    pub proposed_header_row: Option<u32>,
    /// The header row used: the one asked for, or the proposed one.
    pub header_row: Option<u32>,
    /// The columns under the header row.
    pub columns: Vec<PreviewColumn>,
    /// The first twenty rows of the shown sheet, from its top.
    pub rows: Vec<PreviewRowView>,
    /// How many items a batch would read: the non-empty rows below the header.
    pub estimated_items: u64,
    /// True for `.xlsx` and `.xlsm`: results may be written into the original.
    pub can_modify_original: bool,
    /// True for `.xls`, `.xlsb` and `.ods`: the copy of results is a new
    /// workbook without the original formatting.
    pub formatting_not_kept: bool,
}

impl From<TablePreview> for SourcePreviewResponse {
    fn from(preview: TablePreview) -> Self {
        let format = preview.format.extension();
        Self {
            format: format.to_string(),
            source_type: if format == "csv" {
                apollia_sources::CSV_KIND
            } else {
                apollia_sources::SPREADSHEET_KIND
            }
            .to_string(),
            sheets: preview.sheets.into_iter().map(sheet).collect(),
            proposed_sheet: preview.proposed_sheet,
            sheet: preview.sheet,
            proposed_header_row: preview.proposed_header_row,
            header_row: preview.header_row,
            columns: preview.columns.into_iter().map(column).collect(),
            rows: preview.rows.into_iter().map(row).collect(),
            estimated_items: preview.estimated_items,
            can_modify_original: preview.can_modify_original,
            formatting_not_kept: preview.formatting_not_kept,
        }
    }
}

fn sheet(summary: SheetSummary) -> PreviewSheet {
    PreviewSheet {
        name: summary.name,
        non_empty_rows: u64::try_from(summary.non_empty_rows).unwrap_or(u64::MAX),
    }
}

fn column(column: Column) -> PreviewColumn {
    PreviewColumn {
        letter: column.letter,
        name: column.name,
    }
}

fn row(row: PreviewRow) -> PreviewRowView {
    PreviewRowView {
        row: row.row,
        cells: row.cells,
    }
}

/// `POST /api/v1/sources/preview`: the preview of a workbook or a CSV file.
#[utoipa::path(
    post,
    path = "/api/v1/sources/preview",
    tag = "sources",
    request_body = PreviewSourceBody,
    responses(
        (status = 200, description = "The proposed sheet and header row, the first rows and the write flags", body = SourcePreviewResponse),
        (status = 400, description = "The path is not absolute", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "No file at that path", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Not a workbook or a CSV file, protected, damaged, or a sheet or row the file lacks", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn preview_source(
    Json(body): Json<PreviewSourceBody>,
) -> Result<Json<SourcePreviewResponse>, ApiError> {
    let path = PathBuf::from(&body.path);
    if !path.is_absolute() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!("the path '{}' is not absolute", body.path),
        ));
    }
    let PreviewSourceBody {
        sheet, header_row, ..
    } = body;
    let preview =
        tokio::task::spawn_blocking(move || preview_table(&path, sheet.as_deref(), header_row))
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
            .map_err(preview_error)?;
    Ok(Json(preview.into()))
}

fn preview_error(error: PreviewError) -> ApiError {
    let status = match &error {
        PreviewError::Read(ReadError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            StatusCode::NOT_FOUND
        }
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    tracing::event!(
        tracing::Level::INFO,
        status = status.as_u16(),
        error = %error,
        "sources.preview_refused"
    );
    api_err(status, error)
}

#[cfg(test)]
mod tests;

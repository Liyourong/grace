use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result, parse_id},
};

#[derive(Deserialize)]
pub struct FolderName {
    name: String,
}

impl FolderName {
    fn checked(&self) -> Result<&str> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err(AppError::bad_request(
                "invalid_folder_name",
                "Use 1–80 characters for the folder name",
            ));
        }
        Ok(name)
    }
}

fn folder_error(e: tokio_postgres::Error) -> AppError {
    match e.code() {
        Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) => AppError::new(
            StatusCode::CONFLICT,
            "folder_exists",
            "A folder with this name already exists",
        ),
        _ => e.into(),
    }
}
fn folder_missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "not_found", "Folder not found")
}

pub async fn create(
    State(s): State<AppState>,
    user: CurrentUser,
    Body(input): Body<FolderName>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = input.checked()?;
    let row = s
        .pool
        .get()
        .await?
        .query_one(
            "INSERT INTO paper_folders (owner_id,name) VALUES ($1,$2) RETURNING id",
            &[&user.id, &name],
        )
        .await
        .map_err(folder_error)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":row.get::<_,i64>(0).to_string(),"name":name})),
    ))
}

pub async fn rename(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Body(input): Body<FolderName>,
) -> Result<StatusCode> {
    let changed = s
        .pool
        .get()
        .await?
        .execute(
            "UPDATE paper_folders SET name=$3 WHERE id=$1 AND owner_id=$2",
            &[&parse_id(&id)?, &user.id, &input.checked()?],
        )
        .await
        .map_err(folder_error)?;
    if changed == 0 {
        return Err(folder_missing());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    // The composite foreign key moves this owner's papers to root in the same statement.
    let changed = s
        .pool
        .get()
        .await?
        .execute(
            "DELETE FROM paper_folders WHERE id=$1 AND owner_id=$2",
            &[&parse_id(&id)?, &user.id],
        )
        .await?;
    if changed == 0 {
        return Err(folder_missing());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Destination {
    folder_id: Value,
}

pub async fn move_paper(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Body(dest): Body<Destination>,
) -> Result<StatusCode> {
    let folder = match dest.folder_id {
        Value::Null => None,
        Value::String(id) => Some(parse_id(&id)?),
        _ => {
            return Err(AppError::bad_request(
                "invalid_folder",
                "Use a folder ID or null",
            ));
        }
    };
    let changed = s
        .pool
        .get()
        .await?
        .execute(
            "UPDATE papers SET folder_id=$3 WHERE id=$1 AND owner_id=$2",
            &[&parse_id(&id)?, &user.id, &folder],
        )
        .await
        .map_err(|e| {
            if e.code() == Some(&tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION) {
                folder_missing()
            } else {
                e.into()
            }
        })?;
    if changed == 0 {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_paper(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let changed = s
        .pool
        .get()
        .await?
        .execute(
            "DELETE FROM papers WHERE id=$1 AND owner_id=$2",
            &[&parse_id(&id)?, &user.id],
        )
        .await?;
    if changed == 0 {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

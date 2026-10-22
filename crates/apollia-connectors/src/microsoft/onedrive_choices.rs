//! The OneDrive folders and files the user chose for Apollia to read.
//!
//! The `Files.Read` token sees the whole drive. The bound is Apollia's own: an
//! item is readable when it is one of the chosen items, or sits under one of
//! the chosen folders. Every answer of Graph is checked against that list
//! before it reaches an agent, and an item whose place cannot be established
//! is refused rather than admitted.
//!
//! The place of an item is read from Graph, never from the caller: a chosen
//! item records the path Graph gave for it when it was chosen, and an item met
//! later is placed by the `parentReference` Graph returns with it, fetched
//! again by id when the answer left the path out.

use serde::{Deserialize, Serialize};

use crate::error::ConnectorError;
use crate::microsoft::onedrive::{DriveItem, FolderSearch, OneDriveClient};

/// Current version of the persisted shape of a choice.
fn default_format_version() -> u32 {
    1
}

/// Whether a chosen item is a folder, which admits everything under it, or a
/// single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    /// A folder and every item under it.
    Folder,
    /// One file.
    File,
}

/// One folder or file the user chose for Apollia to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneDriveChoice {
    /// Version of the persisted shape. A record without the key reads as 1.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// Graph item id.
    pub id: String,
    /// Name at the time of the choice, for display.
    pub name: String,
    /// Folder or file.
    pub kind: ChoiceKind,
    /// Full path of the item as Graph placed it when it was chosen,
    /// `/drive/root:/Documents/Clients` for instance.
    pub path: String,
    /// Drive holding the item, when Graph named it.
    #[serde(default)]
    pub drive_id: Option<String>,
}

impl OneDriveChoice {
    /// Build the choice of `item`, as Graph described it. An item Graph gave no
    /// parent path for is the root of the drive, or outside it, and cannot be
    /// chosen: choosing the root would open the whole drive.
    pub fn from_item(item: &DriveItem) -> Result<Self, ConnectorError> {
        let reference = item.parent_reference.as_ref();
        let parent = reference.and_then(|r| r.path.as_deref()).ok_or_else(|| {
            ConnectorError::InvalidArgument(format!(
                "`{}` is the root of the drive or has no place in it; choose folders or files inside it",
                item.name
            ))
        })?;
        Ok(Self {
            format_version: default_format_version(),
            id: item.id.clone(),
            name: item.name.clone(),
            kind: if item.folder.is_some() {
                ChoiceKind::Folder
            } else {
                ChoiceKind::File
            },
            path: format!("{}/{}", parent.trim_end_matches('/'), item.name),
            drive_id: reference.and_then(|r| r.drive_id.clone()),
        })
    }
}

/// Where an item stands against the choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The item is chosen, or sits under a chosen folder.
    Inside,
    /// The item is placed, and outside every choice.
    Outside,
    /// The answer did not say where the item sits.
    Unknown,
}

/// The part of a Graph path from `root:` on, decoded and folded to lower case:
/// `/drive/root:/Docs` and `/drives/b!x/root:/Docs` name the same place on the
/// same drive, and OneDrive compares names without case.
fn normalized(path: &str) -> String {
    let from_root = path.find("root:").map_or(path, |at| &path[at..]);
    percent_decode(from_root).to_lowercase()
}

/// The drive a Graph path names, `/drives/<id>/root:...`, if any.
fn drive_in_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/drives/")?;
    rest.split('/').next().filter(|id| !id.is_empty())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where `item` stands against `choices`, from what Graph said about it.
pub fn placement(choices: &[OneDriveChoice], item: &DriveItem) -> Placement {
    if choices.iter().any(|c| c.id == item.id) {
        return Placement::Inside;
    }
    let Some(reference) = item.parent_reference.as_ref() else {
        return Placement::Unknown;
    };
    let Some(parent) = reference.path.as_deref() else {
        return Placement::Unknown;
    };
    let item_drive = reference
        .drive_id
        .as_deref()
        .or_else(|| drive_in_path(parent));
    let full = normalized(&format!("{}/{}", parent.trim_end_matches('/'), item.name));
    let inside = choices.iter().any(|choice| {
        let same_drive = match (choice.drive_id.as_deref(), item_drive) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => true,
        };
        choice.kind == ChoiceKind::Folder
            && same_drive
            && full.starts_with(&format!("{}/", normalized(&choice.path)))
    });
    if inside {
        Placement::Inside
    } else {
        Placement::Outside
    }
}

/// The reason given to an agent for an item outside the choices.
fn outside(name: &str) -> ConnectorError {
    ConnectorError::InvalidArgument(format!(
        "the OneDrive item `{name}` is outside the folders and files chosen in Apollia for this account; ask the user to choose it in Apollia"
    ))
}

/// The reason given when nothing is chosen yet.
fn nothing_chosen() -> ConnectorError {
    ConnectorError::InvalidArgument(
        "no OneDrive folder or file is chosen in Apollia for this account yet; ask the user to choose the ones Apollia may read".into(),
    )
}

/// The OneDrive client, bounded by the choices of one account.
pub struct ChosenOneDrive<'a> {
    client: &'a OneDriveClient,
    choices: &'a [OneDriveChoice],
}

impl<'a> ChosenOneDrive<'a> {
    /// Bound `client` by `choices`.
    pub fn new(client: &'a OneDriveClient, choices: &'a [OneDriveChoice]) -> Self {
        Self { client, choices }
    }

    /// Whether `item` is inside the choices, asking Graph for its place by id
    /// when the answer it came with did not say.
    async fn admits<R, Fut>(&self, item: &DriveItem, bearer: &str, refresh: &mut R) -> bool
    where
        R: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        match placement(self.choices, item) {
            Placement::Inside => true,
            Placement::Outside => false,
            Placement::Unknown => match self
                .client
                .get_metadata(&item.id, bearer, &mut *refresh)
                .await
            {
                Ok(placed) => placement(self.choices, &placed) == Placement::Inside,
                Err(_) => false,
            },
        }
    }

    /// The metadata of `item_id`, refused with a reason when the item is outside
    /// the choices.
    pub async fn get_metadata<R, Fut>(
        &self,
        item_id: &str,
        bearer: &str,
        refresh: &mut R,
    ) -> Result<DriveItem, ConnectorError>
    where
        R: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        if self.choices.is_empty() {
            return Err(nothing_chosen());
        }
        let item = self
            .client
            .get_metadata(item_id, bearer, &mut *refresh)
            .await?;
        if placement(self.choices, &item) == Placement::Inside {
            Ok(item)
        } else {
            Err(outside(&item.name))
        }
    }

    /// The content of `item_id`, refused with a reason when the item is outside
    /// the choices. The content is read by the id Graph answered, so the item
    /// checked is the item read.
    pub async fn download<R, Fut>(
        &self,
        item_id: &str,
        bearer: &str,
        refresh: &mut R,
    ) -> Result<Vec<u8>, ConnectorError>
    where
        R: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let item = self.get_metadata(item_id, bearer, refresh).await?;
        self.client.download(&item.id, bearer, &mut *refresh).await
    }

    /// Search inside each chosen folder, and among the chosen files by name.
    /// Every result is checked against the choices; at most `top` come back.
    pub async fn search<R, Fut>(
        &self,
        query: &str,
        top: u32,
        bearer: &str,
        refresh: &mut R,
    ) -> Result<Vec<DriveItem>, ConnectorError>
    where
        R: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        if self.choices.is_empty() {
            return Err(nothing_chosen());
        }
        let wanted = query.to_lowercase();
        let mut found: Vec<DriveItem> = Vec::new();
        for choice in self.choices {
            if found.len() >= top as usize {
                break;
            }
            let candidates = match choice.kind {
                ChoiceKind::Folder => {
                    self.client
                        .search_in(
                            &FolderSearch {
                                folder_id: &choice.id,
                                query,
                                top,
                            },
                            bearer,
                            &mut *refresh,
                        )
                        .await?
                }
                ChoiceKind::File if choice.name.to_lowercase().contains(&wanted) => {
                    match self
                        .client
                        .get_metadata(&choice.id, bearer, &mut *refresh)
                        .await
                    {
                        Ok(item) => vec![item],
                        Err(_) => Vec::new(),
                    }
                }
                ChoiceKind::File => Vec::new(),
            };
            for item in candidates {
                if found.len() >= top as usize {
                    break;
                }
                if found.iter().any(|f| f.id == item.id) {
                    continue;
                }
                if self.admits(&item, bearer, refresh).await {
                    found.push(item);
                }
            }
        }
        Ok(found)
    }

    /// The recent items of the drive that are inside the choices, at most `top`.
    pub async fn list_recent<R, Fut>(
        &self,
        top: u32,
        bearer: &str,
        refresh: &mut R,
    ) -> Result<Vec<DriveItem>, ConnectorError>
    where
        R: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        if self.choices.is_empty() {
            return Err(nothing_chosen());
        }
        let recent = self.client.list_recent(top, bearer, &mut *refresh).await?;
        let mut kept = Vec::new();
        for item in recent {
            if self.admits(&item, bearer, refresh).await {
                kept.push(item);
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests;

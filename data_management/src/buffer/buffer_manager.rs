use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::{fs::OpenOptions, sync::Mutex};
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

use crate::{DataManagerError, BUFFER_FILE_DIR};

use super::buffer::Buffer;

#[derive(Clone)]
pub struct BufferManager {
    buffer_map: Arc<Mutex<HashMap<i64, Buffer>>>,
}

impl BufferManager {
    pub async fn start() -> Result<Self, DataManagerError> {
        // Open all buffer files
        let root: PathBuf = project_root::get_project_root().unwrap();
        let buffer_file_dir = root.join(BUFFER_FILE_DIR);

        // Create dir if it doesn't exist
        if !buffer_file_dir.exists() {
            tokio::fs::create_dir_all(&buffer_file_dir).await
                .map_err(|_| DataManagerError::BufferManager(format!("Failed to create buffer file directory: {:?}", buffer_file_dir)))?;
        }

        let mut buffer_map = HashMap::new();
        for entry in buffer_file_dir.read_dir().map_err(|_| DataManagerError::BufferManager(format!("Failed to read buffer files from {:?}", buffer_file_dir)))? {
            let path = entry.map(|entry| entry.path())
                .map_err(|_| DataManagerError::BufferManager(format!("Failed to read buffer files from {:?}", buffer_file_dir)))?;

            let Some(session_id) = path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split("_").next())
                .and_then(|prefix| prefix.parse::<i64>().ok()) else {
                return Err(DataManagerError::BufferManager(format!("Data file had illegal path: {:?}", path)));
            };

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .append(true)
                .open(&path).await
                .map_err(|_| DataManagerError::BufferManager(format!("Failed to open buffer file: {:?}", path)))?;

            buffer_map.insert(session_id, Buffer::load(file).await?);
        }

        Ok(BufferManager {
            buffer_map: Arc::new(Mutex::new(buffer_map))
        })
    }

    pub async fn start_session(&self, session: &TrackSession) -> Result<(), DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;

        if session.session_id == -1 {
            return Err(DataManagerError::BufferManager("Session ID must be set".to_string()));
        }

        let root: PathBuf = project_root::get_project_root().unwrap();
        let buffer_file_dir = root.join(BUFFER_FILE_DIR);

        let buffer_file_name = buffer_file_dir.join(format!("{}_{}", session.session_id, session.title));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .create(true)
            .open(&buffer_file_name).await
            .map_err(|_| DataManagerError::BufferManager(format!("Failed to open buffer file: {:?}", buffer_file_name)))?;

        buffer_map.insert(session.session_id, Buffer::new(file, session.start_time).await?);

        Ok(())
    }

    pub async fn append_track_points(&self, session_id: i64, track_points: &[TrackPoint]) -> Result<(), DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;
        let buffer = buffer_map.get_mut(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;
        buffer.add_points(track_points).await?;
        Ok(())
    }

    pub async fn close_session(&self, session_id: i64) -> Result<Vec<TrackPoint>, DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;
        let buffer = buffer_map.remove(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;
        let track_points = buffer.close();

        // delete file
        let root: PathBuf = project_root::get_project_root().unwrap();
        let buffer_file_dir = root.join(BUFFER_FILE_DIR);
        
        // Find file that starts with session_id
        let buffer_file_name = buffer_file_dir.read_dir().map_err(|_| DataManagerError::BufferManager(format!("Failed to read buffer files from {:?}", buffer_file_dir)))?
            .filter_map(|entry| entry.map(|entry| entry.path()).ok())
            .find(|path| path.file_stem()
                             .map(|stem| stem.to_str().unwrap().starts_with(format!("{}_", session_id).as_str()))
                             .unwrap_or(false))
            .ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;

        tokio::fs::remove_file(&buffer_file_name).await.map_err(|_| DataManagerError::BufferManager(format!("Failed to remove buffer file: {:?}", buffer_file_name)))?;

        Ok(track_points)
    }

    pub async fn read_all_track_points(&self, session_id: i64) -> Result<Vec<TrackPoint>, DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;
        let buffer = buffer_map.get_mut(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;
        let track_points = buffer.get_all_track_points().to_vec();
        Ok(track_points)
    }

    pub async fn read_track_points_since(&self, session_id: i64, index: usize) -> Result<Vec<TrackPoint>, DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;
        let buffer = buffer_map.get_mut(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;
        let track_points = buffer.get_track_points_since(index).to_vec();
        Ok(track_points)
    }
}
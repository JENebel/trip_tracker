use std::{collections::HashMap, io::Read, path::PathBuf, sync::Arc};

use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}, sync::Mutex};
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

use crate::{DataManagerError, BUFFER_FILE_DIR};

/**
 * BufferManager is a struct that manages the buffer of track points for active sessions.
 */
#[derive(Clone)]
pub struct BufferManager {
    buffer_map: Arc<Mutex<HashMap<i64, File>>>
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

            let file = File::open(&path).await
                .map_err(|_| DataManagerError::BufferManager(format!("Failed to open buffer file: {:?}", path)))?;

            let Some(session_id) = path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split("_").next())
                .and_then(|prefix| prefix.parse::<i64>().ok()) else {
                return Err(DataManagerError::BufferManager(format!("Data file had illegal path: {:?}", path)));
            };

            buffer_map.insert(session_id, file);
        }

        Ok(BufferManager {
            buffer_map: Arc::new(Mutex::new(buffer_map))
        })
    }

    pub async fn start_session(&self, session: TrackSession) -> Result<(), DataManagerError> {
        if session.session_id == -1 {
            return Err(DataManagerError::BufferManager("Session ID must be set".to_string()));
        }

        let root: PathBuf = project_root::get_project_root().unwrap();
        let buffer_file_dir = root.join(BUFFER_FILE_DIR);

        let buffer_file_name = buffer_file_dir.join(format!("{}_{}", session.session_id, session.title));

        let file = File::create(&buffer_file_name).await
            .map_err(|_| DataManagerError::BufferManager(format!("Failed to create buffer file: {:?}", buffer_file_name)))?;

        let mut buffer_map = self.buffer_map.lock().await;

        buffer_map.insert(session.session_id, file);

        Ok(())
    }

    pub async fn append_track_point(&self, session_id: i64, track_point: TrackPoint) -> Result<(), DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;

        let file = buffer_map.get_mut(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;

        let track_point_bytes = bincode::serialize(&track_point)
            .map_err(|_| DataManagerError::BufferManager("Failed to serialize track point".to_string()))?;

        file.write_all(&track_point_bytes).await.map_err(|_| DataManagerError::BufferManager("Failed to write track point to buffer file".to_string()))?;

        Ok(())
    }

    pub async fn close_session(&self, session_id: i64) -> Result<Vec<TrackPoint>, DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;

        let file = buffer_map.remove(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;

        let mut track_points = Vec::new();

        let mut file = file.into_std().await;

        let mut track_point_bytes = Vec::new();

        file.read_to_end(&mut track_point_bytes).map_err(|_| DataManagerError::BufferManager("Failed to read track points from buffer file".to_string()))?;

        let mut cursor = std::io::Cursor::new(track_point_bytes);

        while let Ok(track_point) = bincode::deserialize_from(&mut cursor) {
            track_points.push(track_point);
        }

        Ok(track_points)
    }

    pub async fn read_buffer(&self, session_id: i64) -> Result<Vec<TrackPoint>, DataManagerError> {
        let mut buffer_map = self.buffer_map.lock().await;

        let file = buffer_map.get_mut(&session_id).ok_or(DataManagerError::BufferManager(format!("No buffer file for session {}", session_id)))?;

        let mut track_points = Vec::new();

        let mut track_point_bytes = Vec::new();

        file.read_to_end(&mut track_point_bytes).await.map_err(|_| DataManagerError::BufferManager("Failed to read track points from buffer file".to_string()))?;

        let mut cursor = std::io::Cursor::new(track_point_bytes);

        while let Ok(track_point) = bincode::deserialize_from(&mut cursor) {
            track_points.push(track_point);
        }

        Ok(track_points)
    }
}
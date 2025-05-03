use std::io::SeekFrom;

use chrono::{DateTime, Utc};
use tokio::{fs::File, io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}};
use trip_tracker_lib::track_point::{TrackPoint, ENCODED_LENGTH};

use crate::DataManagerError;

pub struct Buffer {
    pub start_time: DateTime<Utc>,
    pub track_points: Vec<TrackPoint>,
    pub file: File,
}

impl Buffer {
    pub async fn load(mut file: File) -> Result<Self, DataManagerError> {
        let file_size = file.metadata().await.map_err(|_| DataManagerError::BufferManager("Failed to get metadata for buffer file".to_string()))?.len();

        if file_size < 8 {
            return Err(DataManagerError::BufferManager("Buffer file is too small".to_string()));
        }

        // Start time is the first 8 bytes of the file
        let start_time =  {
            let mut buffer = [0; 8];
            file.seek(SeekFrom::Start(0)).await.map_err(|_| DataManagerError::BufferManager("Failed to seek to track point in buffer file".to_string()))?;
            file.read_exact(&mut buffer).await.map_err(|_| DataManagerError::BufferManager("Failed to read start time from buffer file".to_string()))?;
            let timestamp = i64::from_be_bytes(buffer);
            DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or(DataManagerError::BufferManager(format!("Failed to seek to track point in buffer file: {timestamp} {:?}", &buffer)))?
        };

        let mut track_points = Vec::new();
        let mut buffer = [0; ENCODED_LENGTH];
        for i in (8..file_size as usize).step_by(ENCODED_LENGTH) {
            file.seek(SeekFrom::Start(i as u64)).await.map_err(|_| DataManagerError::BufferManager("Failed to seek to track point in buffer file".to_string()))?;
            file.read_exact(&mut buffer).await.map_err(|_| DataManagerError::BufferManager("Failed to read track point from buffer file".to_string()))?;
            let tp = TrackPoint::from_bytes(&buffer, start_time);
            track_points.push(tp);
        }

        Ok(Self {
            start_time,
            track_points,
            file,
        })
    }

    pub async fn new(mut file: File, start_time: DateTime<Utc>) -> Result<Self, DataManagerError> {
        // Write start time to file
        let buffer = &start_time.timestamp().to_be_bytes();
        file.write_all(buffer).await.map_err(|_| DataManagerError::BufferManager("Failed to write start time to buffer file".to_string()))?;
        file.flush().await.map_err(|_| DataManagerError::BufferManager("Failed to flush buffer file".to_string()))?;

        Ok(Self {
            start_time,
            track_points: Vec::new(),
            file,
        })
    }

    pub fn get_all_track_points(&self) -> &[TrackPoint] {
        &self.track_points
    }

    pub fn get_track_points_since(&self, index: usize) -> &[TrackPoint] {
        &self.track_points[index..]
    }

    pub fn close(self) -> Vec<TrackPoint> {
        self.track_points
    }

    pub async fn add_points(&mut self, new_points: &[TrackPoint]) -> Result<(), DataManagerError> {
        self.track_points.extend_from_slice(new_points);
        self.append_to_file(new_points).await?;
        Ok(())
    }

    async fn append_to_file(&mut self, track_point: &[TrackPoint]) -> Result<(), DataManagerError> {
        self.file.seek(SeekFrom::End(0)).await.map_err(|_| DataManagerError::BufferManager("Failed to seek to start of buffer file".to_string()))?;
        for tp in track_point {
            let bytes = tp.to_bytes(self.start_time);
            self.file.write_all(&bytes).await.map_err(|_| DataManagerError::BufferManager("Failed to write track point to buffer file".to_string()))?;
        }
        self.file.flush().await.map_err(|_| DataManagerError::BufferManager("Failed to flush buffer file".to_string()))?;
        Ok(())
    }
}
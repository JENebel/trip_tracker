use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct SessionUploadStatus {
    pub local_id: u32,
    pub remote_id: Option<i64>,
    pub uploaded: usize,
}

#[derive(Debug, Clone)]
pub struct UploadStatus {
    pub sessions: Vec<SessionUploadStatus>,
}

impl Default for UploadStatus {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }
}

impl UploadStatus {
    // The last line must be the active session, if any
    pub fn parse(input: &str) -> Self {
        let mut lines = input.lines();
        lines.next(); // skip header

        let mut sessions = Vec::new();

        // Read the states
        while let Some(line) = lines.next() {
            let line = line.trim().trim_matches('\0');
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split(',');
            
            let local_id = parts.next().unwrap().trim().parse().unwrap();
            let remote_id = match parts.next().unwrap().trim() {
                "?" => None,
                remote_id => Some(remote_id.parse().unwrap()),
            };
            let uploaded = parts.next().unwrap().trim().parse().unwrap();

            sessions.push(SessionUploadStatus {
                local_id,
                remote_id,
                uploaded,
            });
        }

        Self {
            sessions
        }
    }

    /// Call this when a session has completed uploading. The session will never be visited again.
    pub fn finish_session(&mut self, local_id: u32) {
        self.sessions.retain(|s| s.local_id != local_id);
    }

    pub fn set_remote_session_id(&mut self, local_id: u32, remote_id: i64) {
        for session in self.sessions.iter_mut() {
            if session.local_id == local_id {
                session.remote_id = Some(remote_id);
                return;
            }
        }
    }

    pub fn add_uploaded(&mut self, local_id: u32, uploaded: usize) {
        for session in self.sessions.iter_mut() {
            if session.local_id == local_id {
                session.uploaded += uploaded;
                return;
            }
        }
    }

    pub fn add_session(&mut self, local_id: u32) {
        self.sessions.push(SessionUploadStatus {
            local_id,
            remote_id: None,
            uploaded: 0,
        });
    }

    pub fn get_session_count(&self) -> usize {
        self.sessions.len()
    }
}
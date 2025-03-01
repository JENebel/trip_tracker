use heapless::Vec;

use crate::{ExclusiveService, ModemService, StorageService};

pub enum UploadState {
    Current(usize),
    FromStorage(usize),
}

pub struct UploadService {
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,

    upload_states: Vec<UploadState, 10>,
}

impl UploadService {

}
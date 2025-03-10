use esp_hal::{prelude::nb::block, sha::{Sha, Sha256}};
use trip_tracker_lib::comms::{MacProvider, SIGNATURE_SIZE};

pub struct EmbeddedMacProvider {
    sha: Sha<'static>,
}

impl EmbeddedMacProvider {
    pub fn new(sha: Sha<'static>) -> Self {
        Self {
            sha,
        }
    }
}

impl MacProvider for EmbeddedMacProvider {
    fn sign(&mut self, mut data: &[u8], mut token: &[u8]) -> [u8; SIGNATURE_SIZE] {
        let mut hasher = self.sha.start::<Sha256>();

        let mut output = [0u8; SIGNATURE_SIZE];
        while !data.is_empty() {
            data = block!(hasher.update(data)).unwrap();
        }

        while !token.is_empty() {
            token = block!(hasher.update(token)).unwrap();
        }

        // Finish can be called as many times as desired to get multiple copies of
        // the output.
        block!(hasher.finish(output.as_mut_slice())).unwrap();

        output
    }
}
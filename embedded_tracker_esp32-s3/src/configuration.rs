use core::str::FromStr;

use esp_println::println;
use heapless::String;

#[derive(Debug)]
pub struct Configuration {
    pub sim_pin: String<32>,

    pub apn: String<32>,
    pub apn_user: String<32>,
    pub apn_password: String<32>,

    pub server: String<32>,
    pub port: u16,
    pub trip_id: i64,
    pub auth_key: [u8; 32],
}

impl Configuration {
    pub fn parse(input: &str) -> Self {
        let mut sim_pin = String::default();
        let mut apn = String::default();
        let mut apn_user = String::default();
        let mut apn_password = String::default();
        let mut server = String::default();
        let mut trip_id = -1;
        let mut port = 0;
        let mut auth_key = [0; 32];

        for line in input.split('\n') {
            let line = line.trim();
            if line.is_empty() || line.starts_with("#") {
                continue;
            }

            let mut parts = line.split('=');
            let key = parts.next().unwrap().trim();
            let value = parts.next().unwrap().trim();

            match key {
                "sim_pin" => sim_pin = String::from_str(value).unwrap(),
                "apn" => apn = String::from_str(value).unwrap(),
                "apn_user" => apn_user = String::from_str(value).unwrap(),
                "apn_password" => apn_password = String::from_str(value).unwrap(),
                "server" => server = String::from_str(value).unwrap(),
                "port" => port = u16::from_str(value).unwrap(),
                "trip_id" => trip_id = i64::from_str(value).unwrap(),
                "auth_key" => auth_key = hex_to_bytes(value).unwrap(),
                _ => {
                    println!("Unknown config key: {}", key);
                }
            }
        }
        
        Self {
            sim_pin,
            apn,
            apn_user,
            apn_password,
            server,
            trip_id,
            port,
            auth_key,
        }
    }
}

fn hex_to_bytes<const S: usize>(s: &str) -> Result<[u8; S], ()> {
    let mut bytes = [0; S];
    if s.len() == 2 * S {
        for i in (0..s.len()).step_by(2) {
            let duplet = s.get(i..i + 2).unwrap();
            let byte = u8::from_str_radix(duplet, 16).map_err(|_| ())?;
            bytes[i / 2] = byte;
        }
        Ok(bytes)
    } else {
        Err(())
    }
}
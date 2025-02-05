use core::writeln;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use esp_hal::{uart::UartRx, Async};
use nmea::{Nmea, SentenceType};

const AT_CMD: u8 = 0x0D;

#[embassy_executor::task]
async fn reader(mut rx: UartRx<'static, Async>, signal: &'static Signal<NoopRawMutex, usize>) {
    const MAX_BUFFER_SIZE: usize = 128;

    let mut rbuf: [u8; MAX_BUFFER_SIZE] = [0u8; MAX_BUFFER_SIZE];
    let mut temp_buf: [u8; 1] = [0u8; 1];
    let mut offset = 0;
    loop {
        match embedded_io_async::Read::read_exact(&mut rx, &mut temp_buf).await {
            Ok(_) => {
                // Detect termination; <CR><LF>, 13 = CR, 10 = LF
                if offset > 0 && temp_buf[0] as char == '$' {
                    let sentence = core::str::from_utf8(&rbuf[..offset]).unwrap().trim();
                    signal.signal(offset);
                    handle_nmea_sentence(sentence);
                    offset = 0;
                }

                rbuf[offset] = temp_buf[0];
                offset += 1;
            }
            Err(e) => esp_println::println!("RX Error: {:?}", e),
        }
    }
}

fn handle_nmea_sentence(sentence: &str) {
    let mut nmea = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();
    match nmea.parse(sentence) {
        Ok(nmea::SentenceType::GGA) => {
            if let Some(sats) = nmea.num_of_fix_satellites {
                let x = nmea.latitude;
                let y = nmea.longitude;
                if x.is_some() && y.is_some() {
                    esp_println::println!("Sats: {sats}, pos: {}, {}", x.unwrap(), y.unwrap());
                }
            }
        }
        _ => (),
    }
}
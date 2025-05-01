use std::thread::sleep;
use std::time::Duration;
use std::io::Read;

use sirin_shared::packet::{OutPacket, MAX_OUT_PACKET_SIZE};
use sirin_shared::song::FromSong;
use sirin_shared::{USB_PID, USB_VID};

fn main() {
    loop {
        sleep(Duration::new(0, 1_000_000));
        println!("---");
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();
    
            println!("Bus {:03} Device {:03} ID {:04x}:{:04x}",
                device.bus_number(),
                device.address(),
                device_desc.vendor_id(),
                device_desc.product_id());
            
            if device_desc.vendor_id() == USB_VID && device_desc.product_id() == USB_PID {
                println!("Found Sirin!");
                let mut dev = device.open().unwrap();
                let mut buf = [0u8; MAX_OUT_PACKET_SIZE];
                dev.claim_interface(0).unwrap();
                let len = dev.read_bulk(0, &mut buf, Duration::from_secs(1)).unwrap();
                println!("{:?}", OutPacket::from_song(&buf[0..len]));
            }
        }
    }
} 
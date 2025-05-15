use std::fs::File;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use std::io::Write;

use clap::{Parser, Subcommand, ValueEnum};

use clap_num::maybe_hex;
use derive_more::{Display, From};
use rusb::{Device, DeviceHandle, GlobalContext};
use sirin_shared::mode::SirinMode;
use sirin_shared::packet::{ByteArrayStrError, InPacket, Log, OutPacket, MAX_OUT_PACKET_SIZE};
use sirin_shared::config::{CallsignBuf, NicknameBuf};
use sirin_shared::song::magic::MagicU8;
use sirin_shared::song::{FromSong, FromSongError, SongSize, ToSong, ToSongError};
use sirin_shared::usb::{USB_EP_IN_ADDR, USB_EP_OUT_ADDR, USB_PID, USB_VID};
use sirin_shared::packet::ByteArrayStr;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Subcommands
}

#[derive(Subcommand)]
enum Subcommands {
    #[command(about = "View and set configuration")]
    Config {
        #[arg(long)]
        nickname: Option<String>,

        #[arg(long)]
        callsign: Option<String>,

        #[arg(long, value_parser=maybe_hex::<u16>)]
        id: Option<u16>
    },
    #[command(about = "View and set the mode")]
    Mode {
        mode: Option<Mode>
    },
    #[command(about = "List flights")]
    Flights {},
    #[command(about = "See the outgoing Sirin packets")]
    Tail,
    #[command(about = "List connected Sirins")]
    Ls,
    #[command(about = "Erase everything")]
    Erase,
    #[command(about = "Reboot Sirin")]
    Reboot,
    #[command(about = "Get data from flight")]
    Flight {
        index: u16,
        
        #[arg(long)]
        csv: PathBuf
    }
}

#[derive(ValueEnum, Clone)]
enum Mode {
    Standby,
    Flight
}

fn main() {
    let args = Args::parse();

    match run(args) {
        Ok(_) => {},
        Err(err) => {
            println!("{}", err);
            exit(1)
        }
    }
}

fn run(args: Args) -> Result<(), Error> {
    match args.command {
        Subcommands::Config { nickname, callsign, id } => {
            let is_changing_config = nickname.is_some() || callsign.is_some() || id.is_some();

            let sirin = SirinDev::connect()?.open()?;
            sirin.send_packet(&InPacket::QueryConfig)?;
            
            let mut config = loop {
                let packet = sirin.receive_packet()?;
                let OutPacket::Config(config) = packet else {
                    continue;
                };

                break config;
            };

            if is_changing_config {
                println!("Old config:");
            }
            println!("{}", config);

            if is_changing_config {
                if let Some(nickname) = nickname {
                    config.nickname = NicknameBuf::from_str(&nickname)?;
                }

                if let Some(callsign) = callsign {
                    config.callsign = CallsignBuf::from_str(&callsign)?;
                }

                if let Some(id) = id {
                    config.id = id;
                }

                println!("\nNew config:");
                println!("{}", config);

                sirin.send_packet(&InPacket::SetConfig(config))?;
            }
        }
        Subcommands::Mode { mode } => {
            let sirin = SirinDev::connect()?.open()?;

            if let Some(mode) = mode {
                let mode = match mode {
                    Mode::Standby => SirinMode::Standby,
                    Mode::Flight => SirinMode::Flight
                };

                sirin.send_packet(&InPacket::SetMode(mode))?;

                println!("Set the mode to {}.", mode)
            } else {
                sirin.send_packet(&InPacket::QueryMode)?;

                let mode = loop {
                    let packet = sirin.receive_packet()?;
                    let OutPacket::Mode(mode) = packet else {
                        continue;
                    };

                    break mode;
                };

                println!("{}", mode);
            }
        },
        Subcommands::Flights {  } => {
            let sirin = SirinDev::connect()?.open()?;
            
            sirin.send_packet(&InPacket::QueryFlights)?;

            let mut empty = true;
            for packet in sirin.receive_packets() {
                match packet {
                    Ok(OutPacket::FlightHeader(header)) => {
                        empty = false;
                        println!("{:?}", header)
                    }
                    Err(Error::NoSirin) => return Err(Error::NoSirin),
                    Err(err) => println!("Error: {}", err),
                    _ => {}
                }
            }

            if empty {
                println!("No flights found.");
            }
        }
        Subcommands::Ls => {
            SirinDev::list();
        },
        Subcommands::Tail => {
            let sirin = SirinDev::connect()?.open()?;
            sirin.send_packet(&InPacket::Tail(true))?;
            
            for packet in sirin.receive_packets() {
                match packet {
                    Ok(packet) => println!("{:?}", packet),
                    Err(Error::NoSirin) => return Err(Error::NoSirin),
                    Err(err) => println!("Error: {}", err)
                }
            }
        }
        Subcommands::Erase => {
            let sirin = SirinDev::connect()?.open()?;
            sirin.send_packet(&InPacket::EraseFlash(MagicU8::<0xA8>))?;

            println!("Erasing. This may take some time.");

            for _ in sirin.receive_packets() {}

            println!("Done. Please unplug and plug back in Sirin.");
        }
        Subcommands::Reboot => {
            let sirin = SirinDev::connect()?.open()?;
            sirin.send_packet(&InPacket::Reboot)?;
        }
        Subcommands::Flight { index, csv } => {
            let sirin = SirinDev::connect()?.open()?;
            println!("Writing flight data to {}", csv.to_string_lossy());
            let mut file = File::create(csv)?;
            file.write_all(b"time,pos_x,pos_y,pos_z,vel_x,vel_y,vel_z,accel_x,accel_y,accel_z,altitude\n")?;


            sirin.send_packet(&InPacket::ReadFlight(index))?;
            for packet in sirin.receive_packets() {
                println!("Got packet: {:?}", packet);

                if let OutPacket::LogEntry(entry) = packet? {
                    #[allow(irrefutable_let_patterns)]
                    if let Log::State(state) = entry.log {
                        println!("{:?}", state);
                        writeln!(
                            file,
                            "{},{},{},{},{},{},{},{},{},{},{}",
                            entry.time,
                            state.pos.x.value,
                            state.pos.y.value,
                            state.pos.z.value,
                            state.vel.x.value,
                            state.vel.y.value,
                            state.vel.z.value,
                            state.accel.x.value,
                            state.accel.y.value,
                            state.accel.z.value,
                            state.altitude.value
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

pub struct SirinDev {
    device: Device<GlobalContext>
}

#[derive(From, Display, Debug)]
pub enum Error {
    #[display("No Sirin found.")]
    NoSirin,
    #[display("There was an error with the USB: {_0}")]
    UsbError(rusb::Error),
    #[from]
    #[display("There was an error decoding the packet: {_0:?}")]
    FromSongError(FromSongError),
    #[from]
    #[display("There was an error encoding the packet: {_0:?}")]
    ToSongError(ToSongError),
    #[from]
    #[display("There was an error with a byte array string: {_0:?}")]
    ByteArrayStr(ByteArrayStrError),
    #[from]
    IoError(std::io::Error)
}

impl From<rusb::Error> for Error {
    fn from(value: rusb::Error) -> Self {
        match value {
            rusb::Error::NoDevice => Self::NoSirin,
            err => Self::UsbError(err)
        }
    }
}

impl SirinDev {
    fn list() {
        let mut count = 0;
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() == USB_VID && device_desc.product_id() == USB_PID {
                count += 1;
                let Ok(handler) = device.open() else {
                    continue;
                };

                println!("Bus {:03} Device {:03} ID {:04x}:{:04x} {} {} {}",
                    device.bus_number(),
                    device.address(),
                    device_desc.vendor_id(),
                    device_desc.product_id(),
                    handler.read_manufacturer_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_product_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_serial_number_string_ascii(&device_desc).unwrap_or_default()
                );
            }
        }

        if count == 0 {
            println!("No Sirins found.");
        }
    }

    fn connect() -> Result<Self, Error> {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() == USB_VID && device_desc.product_id() == USB_PID {
                return Ok(Self {
                    device
                })
            }
        }

        return Err(Error::NoSirin)
    }

    pub fn open(&self) -> Result<SirinHandle, Error> {
        let dev = self.device.open()?;
        dev.claim_interface(0)?;
        Ok(SirinHandle { dev })
    }
}

pub struct SirinHandle {
    dev: DeviceHandle<GlobalContext>
}

impl SirinHandle {
    pub fn receive_packet(&self) -> Result<OutPacket, Error> {
        let mut buf = [0u8; MAX_OUT_PACKET_SIZE * 128 * 100];
        let len = self.dev.read_bulk(USB_EP_IN_ADDR, &mut buf, Duration::from_secs(0))?;
        Ok(OutPacket::from_song(&buf[0..len])?)
    }

    pub fn send_packet(&self, packet: &InPacket) -> Result<(), Error> {
        let mut buf = [0u8; MAX_OUT_PACKET_SIZE];
        packet.to_song(&mut buf)?;
        self.dev.write_bulk(USB_EP_OUT_ADDR, &mut buf[0..packet.song_size()], Duration::from_secs(0))?;
        Ok(())
    }

    pub fn receive_packets(&self) -> PacketIterator {
        PacketIterator::new(self)
    }
}

pub struct PacketIterator<'a> {
    handle: &'a SirinHandle,
    is_done: bool
}

impl <'a> PacketIterator<'a> {
    pub fn new(handle: &'a SirinHandle) -> Self {
        PacketIterator {
            handle,
            is_done: false
        }
    }
}

impl <'a> Iterator for PacketIterator<'a> {
    type Item = Result<OutPacket, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None
        }

        let packet = self.handle.receive_packet();

        if let Ok(OutPacket::Ok) = packet {
            self.is_done = true;
            None
        } else if let Ok(OutPacket::Error(err)) = packet {
            self.is_done = true;
            println!("{}", err);
            exit(1)
        } else {
            Some(packet)
        }
    }
}
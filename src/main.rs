use midir::{MidiInput, MidiInputPort};
use buttplug::{
    client::{
        ButtplugClient, ButtplugClientEvent, ButtplugClientDeviceMessageType, 
        VibrateCommand,
    },
    server::ButtplugServerOptions,
    util::async_manager,
};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::runtime::Handle;
use futures::StreamExt;
use futures_timer::Delay;
use std::{error::Error, time::Duration, io::Write};

fn prompt_midi(
    client_name: &str
) -> Result<(MidiInput, MidiInputPort), Box<dyn Error>> {
    let imidi = MidiInput::new(client_name)?;
    let mut iports = imidi.ports();
    match iports.len() {
        0 => Err("no available MIDI input port found".into()),
        1 => {
            println!(
                "selecting only available MIDI input port: {}",
                imidi.port_name(&iports[0])?
            );
            Ok((imidi, iports.pop().unwrap()))
        },
        _ => {
            println!("available input ports:");
            for (i, p) in iports.iter().enumerate() {
                println!("{}: {}", i, imidi.port_name(p)?);
            }
            print!("select input port: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            Ok((
                imidi,
                iports.into_iter().nth(
                    input.trim().parse::<usize>()?
                ).ok_or("invalid input port selected")?
            ))
        },
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    // select MIDI input port
    let (imidi, iport) = prompt_midi("buzzkey midir input")?;
    // connect Buttplug
    let client = ButtplugClient::new("buzzkey buttplug client");
    let mut event_stream = client.event_stream();
    client.connect_in_process(&ButtplugServerOptions::default()).await?;
    // scan for devices
    client.start_scanning().await?;
    async_manager::spawn(async move {
        loop {
            match event_stream.next().await.unwrap() {
                ButtplugClientEvent::DeviceAdded(dev) => {
                    async_manager::spawn(async move {
                        println!("device added: {}", dev.name);
                    }).unwrap();
                },
                ButtplugClientEvent::ScanningFinished => (),
                ButtplugClientEvent::ServerDisconnect => {
                    println!("server disconnected!");
                },
                _ => {
                    println!("something happened!");
                },
            }
        }
    })?;
    println!("scanning started! press enter at any point to stop and start buzzing.");
    BufReader::new(io::stdin()).lines().next_line().await?;
    client.stop_scanning().await?;
    // buzz on MIDI input
    let handle = Handle::current();
    let _iport_connection = imidi.connect(&iport, "buzzkey_iport", move |stamp, message, _| {
        println!("{}: {:?} (len = {})", stamp, message, message.len());
        let status = message[0];
        println!("{:b}", status >> 4);
        if status >> 4 == 0b1001 {
            for dev in client.devices() {
                handle.spawn(async move {
                    if dev.allowed_messages.contains_key(&ButtplugClientDeviceMessageType::VibrateCmd) {
                        dev.vibrate(VibrateCommand::Speed(1.0)).await.unwrap();
                        println!("{} should start vibrating!", dev.name);
                        Delay::new(Duration::from_millis(50)).await;
                        dev.stop().await.unwrap();
                        println!("{} should stop vibrating!", dev.name);
                    } else {
                        println!("{} doesn't vibrate!", dev.name);
                    }
                });
            }
        }
    }, ())?;
    println!("buzzing started! press enter at any point to quit.");
    BufReader::new(io::stdin()).lines().next_line().await?;
    println!("bye-bye! >:3c");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {}!", err);
    };
}

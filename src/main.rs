use midir::MidiInput;
use buttplug::{
    client::{
        ButtplugClient, ButtplugClientDevice, ButtplugClientDeviceMessageType, ButtplugClientEvent,
        VibrateCommand,
    },
    server::ButtplugServerOptions,
    util::async_manager,
};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::runtime::Handle;
use futures::StreamExt;
use futures_timer::Delay;
use std::{sync::Arc, time::Duration, io::Write};
async fn run() {
    // select MIDI input port
    let imidi = MidiInput::new("buzzkey midir input").unwrap();
    let iports = imidi.ports();
    let iport = match iports.len() {
        0 => {
            println!("no available MIDI input port found!");
            return;
        },
        1 => {
            println!("only available MIDI input port: {}", imidi.port_name(&iports[0]).unwrap());
            &iports[0]
        },
        _ => {
            println!("available MIDI input ports:");
            for (i, p) in iports.iter().enumerate() {
                println!("{}: {}", i, imidi.port_name(p).unwrap());
            }
            print!("select MIDI input port: ");
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            iports.get(input.trim().parse::<usize>().unwrap()).unwrap()
        }
    };
    // connect Buttplug
    let client = ButtplugClient::new("buzzkey buttplug client");
    let mut event_stream = client.event_stream();
    client.connect_in_process(&ButtplugServerOptions::default()).await.unwrap();
    // scan for devices
    if let Err(err) = client.start_scanning().await {
        println!("buzzkey client failed to start scan!: {}", err);
        return;
    }
    async_manager::spawn(async move {
        loop {
            match event_stream.next().await.unwrap() {
                ButtplugClientEvent::DeviceAdded(dev) => {
                    async_manager::spawn(async move {
                        println!("device added: {}", dev.name);
                    }).unwrap();
                }
                ButtplugClientEvent::ScanningFinished => (),
                ButtplugClientEvent::ServerDisconnect => {
                    println!("server disconnected!");
                }
                _ => {
                    println!("something happened!");
                }
            }
        }
    }).unwrap();
    println!("scanning started! press enter at any point to stop and start buzzing.");
    BufReader::new(io::stdin()).lines().next_line().await.unwrap();
    client.stop_scanning().await.unwrap();
    // buzz on MIDI input
    let handle = Handle::current();
    let _iport_connection = imidi.connect(iport, "midi-chaos_iport", move |stamp, message, _| {
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
    }, ()).unwrap();
    println!("buzzing started! press enter at any point to quit.");
    BufReader::new(io::stdin()).lines().next_line().await.unwrap();
    println!("bye-bye! >:3c");
}

#[tokio::main]
async fn main() {
    run().await;
}

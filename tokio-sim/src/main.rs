use tokio::{
    runtime::Runtime,
    sync::mpsc::{channel, unbounded_channel},
    time::*,
    tprintln,
};

use std::cmp::Reverse;
use std::collections::BinaryHeap;


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Event {
    time: SimTime,
    typ: usize, // 0 - wakeup, 1 - message
}

fn main() {

    let rt = Runtime::new().unwrap();

    let (tx, mut rx) = channel::<usize>(32);

    let _handle = rt.spawn(async move {
        tprintln!("Started recv");
        while let Some(i) = rx.recv().await {
            tprintln!("Recived packet {}", i);
            sleep(Duration::new(1, 0)).await;
            tprintln!("Sleep completed for {}", i);
        }
    });

    let mut events = BinaryHeap::<Reverse<Event>>::new();

    events.push(Reverse(Event {
        time: 1.0.into(),
        typ: 1,
    }));
    events.push(Reverse(Event {
        time: 5.0.into(),
        typ: 5,
    }));
    events.push(Reverse(Event {
        time: 10.0.into(),
        typ: 10,
    }));

    // This event may occur at the 10.5 minute mark
    // but will not be processed by the task until the
    // sleep has commenced thus until 11.0
    events.push(Reverse(Event {
        time: 10.5.into(),
        typ: 11,
    }));

    while let Some(Reverse(event)) = events.pop() {
        println!("SIM: {:?}", event);

        SimTime::set_now(event.time);
        rt.poll_time_events();

        match event.typ {
            0 => {
                rt.poll_until_idle();
            }
            _ => {
                let my_tx = tx.clone();
                let _v = rt.block_or_idle_on(async move { my_tx.send(event.typ).await });
                println!("{:?}", _v);
                rt.poll_until_idle();
            }
        }

        // Run Runtime
        let next_wakeup = rt.next_time_poll();
        if let Some(next_wakeup) = next_wakeup {
            tprintln!("Inserting wakeup at: {}", next_wakeup);
            events.push(Reverse(Event {
                time: next_wakeup,
                typ: 0,
            }));
        }
    }

    drop(tx);
}

fn rt_channel_main() {
    let rt = Runtime::new().expect("Failed to create runtime");

    // General setup
    // ===
    //
    // Tasks:
    // - Central Mangaer
    // - SubManagerAlpha
    // - SubManagerBeta
    //
    // Connections:
    // - upstream
    // - downstream
    // - main-alapha
    // - alpha-main
    // - main-beta

    let (up_tx, mut up_rx) = channel(32);
    let (down_tx, mut down_rx) = unbounded_channel();

    let (ma_tx, mut ma_rx) = channel(16);
    let (am_tx, mut am_rx) = channel(16);
    let (mb_tx, mut mb_rx) = channel(16);

    let _handle_main = rt.spawn(async move {
        while let Some(v) = up_rx.recv().await {
            println!("[Main] Received value {} forwarding to Alpha", v);
            let v: usize = v;
            // Send it to alpha
            ma_tx.send(v).await.unwrap();
            // listen for result
            let result = am_rx.recv().await.unwrap();
            println!("[Main] Received value {} forwarding to Beta / Down", result);
            // send to beta and down
            mb_tx.send(result).await.unwrap();
            down_tx.send(result).unwrap();
        }
    });

    let _handle_alpha = rt.spawn(async move {
        while let Some(v) = ma_rx.recv().await {
            println!("[Alpha] Received value {} sending {}", v, v * 2);
            am_tx.send(v * 2).await.unwrap();
        }
    });

    let _handle_beta = rt.spawn(async move {
        while let Some(v) = mb_rx.recv().await {
            println!("[Beta] Received value {}", v)
        }
    });

    for v in [1, 2, 3] {
        let my_up_tx = up_tx.clone();
        rt.block_on(async move {
            my_up_tx.send(v).await.unwrap();
        });

        rt.poll_until_idle();
        let mut results = Vec::new();
        while let Ok(v) = down_rx.try_recv() {
            results.push(v)
        }

        println!("{:?}", results)
    }
}

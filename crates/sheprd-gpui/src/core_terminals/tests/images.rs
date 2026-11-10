//! Images a herdr-core terminal places: its emulator sends each image's
//! bytes once, and the worker keeps them for every screen that places it.

use super::worker::{Scripted, size, until};
use crate::core_terminals::worker::{self, Command};
use sheprd_backend::{Role, SurfaceUpdate, TerminalId};
use sheprd_client::protocol::{
    SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat, SurfaceGraphicsPlacement,
    SurfaceGraphicsScene, SurfaceGraphicsSource, SurfaceGraphicsTarget,
};

fn key() -> SurfaceGraphicsAssetKey {
    SurfaceGraphicsAssetKey {
        source: SurfaceGraphicsSource::Terminal {
            target: SurfaceGraphicsTarget::Pane {
                pane_id: String::new(),
            },
            image_id: 1,
        },
        image_width: 1,
        image_height: 1,
        format: SurfaceGraphicsFormat::Rgb,
        data_len: 3,
        data_fingerprint: 9,
    }
}

fn placed(y: u16) -> SurfaceGraphicsPlacement {
    SurfaceGraphicsPlacement {
        asset: key(),
        logical_placement_id: 0,
        x: 0,
        y,
        cols: 1,
        rows: 1,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        x_offset: 0,
        y_offset: 0,
        z: 0,
        scrollback_offset: 0,
    }
}

#[test]
fn image_bytes_sent_once_stay_for_every_screen_that_places_them() {
    let backend = Scripted::default();
    backend.push(vec![SurfaceUpdate::Graphics(SurfaceGraphicsScene {
        assets: vec![SurfaceGraphicsAsset {
            key: key(),
            data: vec![1, 2, 3],
        }],
        placements: vec![placed(0)],
        retained_assets: Vec::new(),
    })]);
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(10, 4),
        Role::Active,
    )
    .unwrap();
    let first = until(|| link.mailbox.take());
    assert_eq!(first.graphics.len(), 1);
    assert_eq!(first.images.0.get(&key()).unwrap().data()[..], [1, 2, 3]);
    // Scrolled: the same image, no bytes this time.
    backend.push(vec![SurfaceUpdate::Graphics(SurfaceGraphicsScene {
        assets: Vec::new(),
        placements: vec![placed(2)],
        retained_assets: Vec::new(),
    })]);
    let moved = until(|| link.mailbox.take());
    assert_eq!(moved.graphics[0].y, 2);
    assert!(moved.images.0.get(&key()).is_some());
    // Gone from the screen: the bytes go too.
    backend.push(vec![SurfaceUpdate::Graphics(
        SurfaceGraphicsScene::default(),
    )]);
    let gone = until(|| link.mailbox.take());
    assert!(gone.graphics.is_empty());
    assert!(gone.images.0.get(&key()).is_none());
}

#[test]
fn the_cell_size_reaches_the_terminal() {
    let backend = Scripted::default();
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(10, 4),
        Role::Active,
    )
    .unwrap();
    link.send(Command::CellPixels(9, 18));
    until(|| {
        backend
            .received
            .lock()
            .unwrap()
            .contains(&Command::CellPixels(9, 18))
            .then_some(())
    });
}

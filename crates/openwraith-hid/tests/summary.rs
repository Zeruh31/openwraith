//! `read_summary` (the reads behind `openwraith info`) against the captures.

mod common;

use common::{C06, assert_only_reads, mock, unknown_model_mock};
use openwraith_hid::protocol::{ReadRequest, W75};
use openwraith_hid::{BackupError, Session, read_summary};

#[test]
fn summary_of_capture_06() {
    let mut session = Session::new(mock(C06));
    let summary = read_summary(&mut session).expect("summary of c06");
    assert_eq!(summary.device.model.as_deref(), Some("Wraith W75"));
    assert_eq!(summary.device.product.as_deref(), Some("W75HEARGB"));
    assert_eq!(summary.device.firmware.as_deref(), Some("3.22.07"));
    assert_eq!(summary.model(), Some(&W75));
    let usb = summary.device.usb.as_ref().expect("usb identity from OPEN");
    assert_eq!((usb.vendor_id, usb.product_id), (0x2E3C, 0xC368));
    assert_eq!(summary.active_profile, 1);
    assert_eq!(summary.poll_rate.as_ref().and_then(|p| p.hz), Some(8000));
    assert_eq!(summary.trigger_global.max_travel.to_string(), "3.40 mm");
    assert_eq!(summary.trigger_global.min_travel.to_string(), "0.10 mm");
    assert!(summary.status.socd_supported);

    // Exactly the five reads, in this order.
    let mock = session.into_transport();
    let sent: Vec<_> = mock
        .written()
        .iter()
        .map(|p| ReadRequest::from_packet(p.as_bytes()))
        .collect();
    assert_eq!(
        sent,
        [
            ReadRequest::DeviceInfo,
            ReadRequest::Status,
            ReadRequest::ActiveProfile,
            ReadRequest::TriggerGlobal,
            ReadRequest::PollRate,
        ]
        .map(Some)
    );
    assert_only_reads(&mock);

    // Serialisable for a GUI.
    let json = serde_json::to_value(&summary).expect("serialises");
    assert_eq!(json["device"]["product"], "W75HEARGB");
    assert_eq!(json["active_profile"], 1);
}

#[test]
fn summary_refuses_unknown_models_after_the_device_info() {
    let mut session = Session::new(unknown_model_mock());
    let err = read_summary(&mut session).unwrap_err();
    assert!(
        matches!(&err, BackupError::UnsupportedModel { product: Some(p), .. } if p == "AP61HEARGB"),
        "{err}"
    );
    // Nothing but the device-info request was sent.
    assert_eq!(session.transport().written().len(), 1);
    assert_only_reads(session.transport());
}

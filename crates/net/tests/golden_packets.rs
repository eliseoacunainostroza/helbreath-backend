use net::{
    decode_frame, encode_frame, translate_packet, SessionPhase, OPCODE_CHAT, OPCODE_HEARTBEAT,
};

#[test]
fn golden_chat_packet_translates() {
    let frame = encode_frame(OPCODE_CHAT, b"hello-world");
    let decoded = decode_frame(&frame, 4096).expect("decode");
    let cmd = translate_packet(&decoded, SessionPhase::InWorld).expect("translate");

    match cmd {
        application::ClientCommand::Chat { message } => assert_eq!(message, "hello-world"),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn golden_heartbeat_translates() {
    let frame = encode_frame(OPCODE_HEARTBEAT, &[]);
    let decoded = decode_frame(&frame, 4096).expect("decode");
    let cmd = translate_packet(&decoded, SessionPhase::InWorld).expect("translate");

    assert!(matches!(cmd, application::ClientCommand::Heartbeat));
}

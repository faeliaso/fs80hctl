#![no_main]

use libfuzzer_sys::fuzz_target;
use fs80h_driver::frame::{parse_raw_frame, FrameAssembler};
use fs80h_driver::protocol::parse_response_packet;

fuzz_target!(|data: &[u8]| {
    // Fuzz frame parsing
    let _ = parse_raw_frame(data);

    // Fuzz protocol response packet parser
    let _ = parse_response_packet(data);

    // Fuzz chunk assembly
    let mut assembler = FrameAssembler::new();
    let _ = assembler.append_chunk(data);
    let _ = assembler.finalize();
});

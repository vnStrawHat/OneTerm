//! A terminal with no window: feed bytes, read the events, print the screen.
//!
//! Run it with `cargo run -p oneterm-vt --example headless`.

use std::time::Instant;

use oneterm_vt::{
    CellWidth, Config, EventBatch, OscClaims, RenderContent, RenderRow, RenderState, Size,
    Terminal, VtEvent,
};

/// Everything below arrives in one `feed`: a title, a working directory, an
/// application-private OSC, colour, and two cursor moves.
const INPUT: &[u8] = b"\x1b]0;headless demo\x07\
    \x1b]7;file://localhost/tmp\x1b\\\
    \x1b]1337;SetUserVar=demo\x07\
    \x1b[1;32mhello\x1b[0m world\
    \x1b[3;3Hrow three";

fn main() {
    // Claim the two OSC numbers this program answers itself. Anything the
    // engine handles natively still arrives as a typed event, so claiming is
    // only ever about sequences the engine has no opinion on.
    let mut claims = OscClaims::new();
    claims.claim(7).claim(1337);

    let mut term = Terminal::new(
        Size { rows: 4, cols: 32 },
        Config {
            osc_claims: claims,
            // What `XTVERSION` and `DA2` will tell programs they are talking to.
            product_name: Some("headless-demo(1.0.0)".into()),
            ..Config::default()
        },
    );

    // The batch is reusable: clear it and feed again, and no allocation
    // happens after the first few chunks.
    let mut batch = EventBatch::new();
    let stats = term.feed(INPUT, &mut batch, Instant::now());

    println!("-- events --");
    for event in batch.iter() {
        match event {
            VtEvent::Title(span) => println!("title      {:?}", batch.str(*span)),
            VtEvent::Osc { code, params, .. } => {
                // Parameter 0 is the OSC number itself, which `code` already
                // carries, so the payload starts at 1.
                let text: Vec<String> = batch
                    .params(*params)
                    .skip(1)
                    .map(|param| String::from_utf8_lossy(param).into_owned())
                    .collect();
                println!("osc {code:<6} {:?}", text.join(";"));
            }
            other => println!("{other:?}"),
        }
    }
    println!(
        "{} bytes fed, {} unhandled sequences",
        stats.bytes, stats.unhandled_sequences
    );

    // Drawing is a pull: ask when you want to, and get back only what changed
    // since this `RenderState` last asked.
    let mut state = RenderState::new();
    let update = term.render_update(&mut state, Instant::now());

    println!("-- screen ({update:?}) --");
    for row in state.rows() {
        println!("|{}|", row_text(row));
    }
}

/// One row as plain text, wide-glyph spacers dropped and clusters expanded.
fn row_text(row: &RenderRow) -> String {
    let mut text = String::new();
    for cell in &row.cells {
        if cell.width == CellWidth::WideSpacer {
            continue;
        }
        match cell.content {
            RenderContent::Scalar(c) => text.push(c),
            RenderContent::Cluster { start, len } => text.extend(row.cluster(start, len)),
        }
    }
    text
}

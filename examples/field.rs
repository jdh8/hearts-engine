//! Replay the Hearts Bot Arena's public games through the real rules.
//!
//! ```console
//! scripts/arena-export.py hands DIR | cargo run --release --example field
//! ```
//!
//! Stage 0 of the field clone (docs/arena-gap.md, P5): every decision in
//! the mirror must be reachable as the [`View`] its seat had, and every
//! hand must settle as the site scored it.  Each stdin line is one hand in
//! the format `scripts/arena-export.py` documents; a hand whose record the
//! rules reject is reported and counted as diverged, and any divergence
//! fails the run.  The per-bot table is the decision count a clone of that
//! bot can train on.

use anyhow::{Context as _, Result, ensure};
use hearts_engine::hearts::{Card, Hand, PassDirection, Round, Rules, Seat};
use hearts_engine::{Strategy, Table, View};
use std::collections::BTreeMap;
use std::io::BufRead as _;

/// Plays back one recorded decision.
struct Script {
    pass: [Card; 3],
    play: Card,
}

impl Strategy for Script {
    fn pass_cards(&mut self, _: &View<'_>) -> [Card; 3] {
        self.pass
    }

    fn play_card(&mut self, _: &View<'_>) -> Card {
        self.play
    }
}

/// Concatenated two-character codes, e.g. `KCKHQC`.
fn cards(text: &str) -> Result<Vec<Card>> {
    text.as_bytes()
        .chunks(2)
        .map(|code| Ok(std::str::from_utf8(code)?.parse()?))
        .collect()
}

fn four<T: std::str::FromStr>(fields: &[&str]) -> Result<[T; 4]> {
    let parsed: Option<Vec<T>> = fields.iter().map(|field| field.parse().ok()).collect();
    parsed
        .and_then(|values| values.try_into().ok())
        .context("four numbers")
}

/// Replay one hand, crediting each decision to the bot that made it as
/// `[passes, plays]`.
fn replay(fields: &[&str], tally: &mut BTreeMap<String, [u64; 2]>) -> Result<()> {
    ensure!(fields.len() == 25, "{} fields", fields.len());
    let direction: PassDirection = fields[2].parse()?;
    let bots = &fields[3..7];
    let mut dealt = [Hand::EMPTY; 4];
    for (hand, text) in dealt.iter_mut().zip(&fields[11..15]) {
        *hand = cards(text)?.into_iter().collect();
    }
    let round = Round::from_deal(Rules::new(), direction, dealt)?;
    let mut table = Table::new(round).scores(four(&fields[7..11])?);
    let mut script = Script {
        pass: [Card::TWO_OF_CLUBS; 3],
        play: Card::TWO_OF_CLUBS,
    };

    if direction != PassDirection::Hold {
        for seat in Seat::ALL {
            ensure!(table.turn() == Some(seat), "{seat} is not next to pass");
            let pass = cards(fields[15 + seat as usize])?;
            let hand = table.view(seat).hand();
            ensure!(
                pass.iter().all(|&card| hand.contains(card)),
                "pass not held"
            );
            script.pass = pass.try_into().ok().context("three cards to pass")?;
            table.step(&mut script)?;
            tally.entry(bots[seat as usize].into()).or_default()[0] += 1;
        }
    }

    for play in fields[19].as_bytes().chunks(3) {
        let seat = *Seat::ALL
            .get(usize::from(play[0].wrapping_sub(b'0')))
            .context("seat digit")?;
        script.play = std::str::from_utf8(&play[1..])?.parse()?;
        ensure!(table.turn() == Some(seat), "{seat} is not on turn");
        ensure!(
            table.view(seat).legal_plays().contains(script.play),
            "{} is not legal for {seat}",
            script.play
        );
        table.step(&mut script)?;
        tally.entry(bots[seat as usize].into()).or_default()[1] += 1;
    }

    let result = table.round().result().context("the hand did not finish")?;
    ensure!(
        result.scores(&Rules::new()) == four(&fields[20..24])?,
        "the site scored it differently"
    );
    let shooter = result.shooter().map(|seat| (seat as u8).to_string());
    ensure!(
        shooter.as_deref().unwrap_or("-") == fields[24],
        "the site names another shooter"
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut tally = BTreeMap::new();
    let (mut hands, mut diverged, mut games, mut last) = (0u64, 0u64, 0u64, String::new());
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.split(' ').collect();
        hands += 1;
        if fields[0] != last {
            games += 1;
            last = fields[0].into();
        }
        if let Err(error) = replay(&fields, &mut tally) {
            diverged += 1;
            if diverged <= 10 {
                let hand = fields.get(1).copied().unwrap_or("?");
                eprintln!("game {} hand {hand}: {error:#}", fields[0]);
            }
        }
    }

    println!("{games} games, {hands} hands, {diverged} diverged");
    println!("\n{:<24}{:>10}{:>12}", "bot", "passes", "plays");
    let mut rows: Vec<_> = tally.into_iter().collect();
    rows.sort_by_key(|&(_, [_, plays])| std::cmp::Reverse(plays));
    for (bot, [passes, plays]) in rows {
        println!("{bot:<24}{passes:>10}{plays:>12}");
    }
    ensure!(diverged == 0, "{diverged} hands diverged");
    Ok(())
}

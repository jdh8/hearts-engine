//! Play Hearts against three bots in the terminal.
//!
//! ```console
//! cargo run --release --example play
//! cargo run --release --example play -- --bot mc:64 --seed 7
//! ```
//!
//! You are seated South and see only legal information: your hand, the
//! public tricks, and the score sheet.  Cards parse in either compact or
//! Unicode form (`QS`, `Q♠`).  On passing deals, type the three cards to
//! send as a spaced list; in play, type one card from the legal set shown.
//! Type `hint` at either prompt for the Monte Carlo solver's read on the
//! current decision: every candidate move with its chance to win the game
//! and expected round points (lower is better).

use anyhow::{Context as _, Result, bail};
use hearts_engine::hearts::{Card, Hand, Phase, Rank, Rules, Seat, Suit};
use hearts_engine::{Assessment, EngineError, HeuristicBot, MonteCarloBot, Strategy, Table, View};
use rand::rngs::StdRng;
use rand::{RngExt as _, SeedableRng};
use std::io::Write as _;

const HUMAN: Seat = Seat::South;

fn usage() {
    println!("Usage: play [--bot newbie|greedy|mc[:N]] [--seed N]");
}

fn parse_args() -> Result<Option<(String, Option<u64>)>> {
    let mut bot = "mc".to_string();
    let mut seed = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--bot" => bot = value()?,
            "--seed" => seed = Some(value()?.parse()?),
            "--help" | "-h" => return Ok(None),
            other => bail!("unknown flag {other:?} (--bot/--seed)"),
        }
    }
    Ok(Some((bot, seed)))
}

/// A newcomer that chooses uniformly from the legal cards, with no memory
/// and no idea which cards are dangerous.
struct NewbieBot {
    rng: StdRng,
}

impl Strategy for NewbieBot {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        let mut cards: Vec<Card> = view.hand().into_iter().collect();
        for i in (1..cards.len()).rev() {
            let j = self.rng.random_range(0..=i);
            cards.swap(i, j);
        }
        [cards[0], cards[1], cards[2]]
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        let cards: Vec<Card> = view.legal_plays().into_iter().collect();
        cards[self.rng.random_range(0..cards.len())]
    }

    fn name(&self) -> &str {
        "newbie"
    }
}

fn make_bot(spec: &str, rng: &mut StdRng) -> Result<Box<dyn Strategy>> {
    let (kind, samples) = match spec.split_once(':') {
        Some((kind, samples)) => (kind, Some(samples.parse::<u32>()?)),
        None => (spec, None),
    };
    match kind {
        "newbie" if samples.is_none() => Ok(Box::new(NewbieBot {
            rng: StdRng::seed_from_u64(rng.random()),
        })),
        "greedy" if samples.is_none() => Ok(Box::new(HeuristicBot::new())),
        "mc" => Ok(Box::new(
            MonteCarloBot::new(StdRng::seed_from_u64(rng.random())).samples(samples.unwrap_or(64)),
        )),
        "newbie" | "greedy" => bail!("{kind} does not take a sample count"),
        other => bail!("unknown bot {other:?} (newbie | greedy | mc[:samples])"),
    }
}

/// Read one trimmed lowercase line, or `None` on end of input.
fn read_command(prompt: &str) -> Option<String> {
    print!("{prompt} ");
    std::io::stdout().flush().ok()?;
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        // EOF (Ctrl-D) leaves the cursor mid-prompt; close the line first.
        Ok(0) | Err(_) => {
            println!();
            None
        }
        Ok(_) => Some(line.trim().to_lowercase()),
    }
}

/// Resolve user text to a card.  A full name (`QS`, `Q♠`) is taken as
/// written; a lone rank (`5`) or suit (`♠`, `s`) resolves only when the hand
/// holds exactly one matching card, so the common case needs no full name.
fn resolve_card(text: &str, hand: Hand) -> Option<Card> {
    if let Ok(card) = text.parse::<Card>() {
        return Some(card);
    }
    let matches: Vec<Card> = if let Ok(rank) = text.parse::<Rank>() {
        hand.into_iter().filter(|card| card.rank == rank).collect()
    } else if let Ok(suit) = text.parse::<Suit>() {
        hand.into_iter().filter(|card| card.suit == suit).collect()
    } else {
        Vec::new()
    };
    match matches.as_slice() {
        [card] => Some(*card),
        [] => {
            println!("Cannot read {text:?} as a card; try forms like QS or Q♠.");
            None
        }
        many => {
            let names: Vec<String> = many.iter().map(Card::to_string).collect();
            println!("{text:?} matches several cards: {}.", names.join(" "));
            None
        }
    }
}

/// A card set as a spaced list, friendlier than the dotted suit groups used
/// by `Hand`'s compact notation.
fn list(cards: Hand) -> String {
    cards
        .into_iter()
        .map(|card| card.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Show everything the human may see before a decision.
fn show_position(view: &View<'_>) {
    println!();
    println!("Your hand: {}", list(view.hand()));
    if let Some(trick) = view.current_trick()
        && !trick.is_empty()
    {
        println!("Trick: {trick}");
    }
    let scores = view.game_scores();
    println!(
        "Score: you {} | W {} | N {} | E {}",
        scores[0], scores[1], scores[2], scores[3],
    );
}

/// Print the Monte Carlo solver's read on the current decision: each
/// candidate with its win-the-game equity and expected penalty points,
/// ranked by equity and the bot's own pick highlighted.
fn print_hints(rows: &[Assessment]) {
    if rows.is_empty() {
        println!("Nothing to weigh here — the move is forced.");
        return;
    }
    println!("Solver — equity is your chance to win the game; lower EV is better:");
    if !rows[0].recommended {
        println!("(Equities within the sampling noise are ties; the bot holds its default move.)");
    }
    for row in rows {
        let mark = if row.recommended { "▸" } else { " " };
        let line = format!(
            "  {mark} {:<22} {:>5.1}%   EV {:>4.1} points",
            row.action,
            row.equity * 100.0,
            row.ev,
        );
        if row.recommended {
            println!("\x1b[7m{line}\x1b[0m");
        } else {
            println!("{line}");
        }
    }
}

/// Interactive human seat, with a private solver seeded apart from the deal
/// so asking for a hint never perturbs the game's own randomness.
struct HumanCli {
    hinter: MonteCarloBot<StdRng>,
}

impl Strategy for HumanCli {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        show_position(view);
        println!("Pass three cards {}.", view.direction());
        loop {
            let command = read_command("Your pass [e.g. QS AH 2C / hint / quit]:")
                .unwrap_or_else(|| "quit".into());
            match command.as_str() {
                "hint" | "h" => print_hints(&self.hinter.assess(view)),
                "quit" => std::process::exit(0),
                text => {
                    let names: Vec<&str> = text.split_whitespace().collect();
                    if names.len() != 3 {
                        println!("Choose exactly three cards, separated by spaces.");
                        continue;
                    }
                    let Some(one) = resolve_card(names[0], view.hand()) else {
                        continue;
                    };
                    let Some(two) = resolve_card(names[1], view.hand()) else {
                        continue;
                    };
                    let Some(three) = resolve_card(names[2], view.hand()) else {
                        continue;
                    };
                    let picks: Hand = [one, two, three].into_iter().collect();
                    if picks.len() != 3 {
                        println!("Choose three distinct cards.");
                    } else if picks - view.hand() != Hand::EMPTY {
                        println!("Every passed card must be in your hand.");
                    } else {
                        return [one, two, three];
                    }
                }
            }
        }
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        show_position(view);
        println!("Legal: {}", list(view.legal_plays()));
        loop {
            let command =
                read_command("Your play [<card> / hint / quit]:").unwrap_or_else(|| "quit".into());
            match command.as_str() {
                "hint" | "h" => print_hints(&self.hinter.assess(view)),
                "quit" => std::process::exit(0),
                text => {
                    let Some(card) = resolve_card(text, view.hand()) else {
                        continue;
                    };
                    if view.legal_plays().contains(card) {
                        return card;
                    }
                    println!(
                        "{card} is not legal here; choose from {}.",
                        list(view.legal_plays())
                    );
                }
            }
        }
    }

    fn name(&self) -> &str {
        "you"
    }
}

fn score_line(scores: [u16; 4]) -> String {
    format!(
        "N {:>3} | E {:>3} | you {:>3} | W {:>3}",
        scores[0], scores[1], scores[2], scores[3],
    )
}

fn main() -> Result<()> {
    let Some((spec, seed)) = parse_args()? else {
        usage();
        return Ok(());
    };
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let mut bots: [Option<Box<dyn Strategy>>; 4] = [
        Some(make_bot(&spec, &mut rng)?),
        Some(make_bot(&spec, &mut rng)?),
        None,
        Some(make_bot(&spec, &mut rng)?),
    ];
    let mut human = HumanCli {
        hinter: MonteCarloBot::new(StdRng::seed_from_u64(seed.unwrap_or(0))).samples(128),
    };
    let rules = Rules::new();
    let mut game = hearts_engine::hearts::Game::new(rules);

    println!(
        "Hearts to {} points — you (South) vs three {spec} bots.",
        rules.game_target,
    );

    while !game.is_over() {
        let number = game.deals() + 1;
        println!();
        println!("=== Round {number} (pass {}) ===", game.next_direction());
        let mut table = Table::new(game.deal(&mut rng)).scores(game.scores());

        while let Some(seat) = table.turn() {
            let phase = table.round().phase();
            let played = table.round().played();
            let tricks = table.round().tricks().len();
            let step = if seat == HUMAN {
                table.step(&mut human)
            } else {
                let bot = bots[seat as usize]
                    .as_deref_mut()
                    .expect("every non-human seat has a bot");
                table.step(bot)
            };
            match step {
                Ok(_) => {}
                Err(EngineError::IllegalAction { source, .. }) if seat == HUMAN => {
                    println!("Illegal: {source}");
                    continue;
                }
                Err(error) => return Err(error.into()),
            }

            if seat != HUMAN {
                match phase {
                    Phase::Passing => println!("{seat} passes."),
                    Phase::Playing => {
                        let card = (table.round().played() - played)
                            .into_iter()
                            .next()
                            .expect("one card was played");
                        println!("{seat} plays {card}.");
                    }
                    Phase::Finished => {}
                }
            }
            if table.round().tricks().len() > tricks {
                let trick = table.round().tricks().last().expect("a completed trick");
                let winner = trick.winner().expect("a completed trick has a winner");
                let points = trick.points();
                let suffix = if points == 1 { "" } else { "s" };
                println!("Trick: {trick} — {winner} takes {points} point{suffix}.");
            }
        }

        let result = table.round().result().expect("a turnless round finished");
        game.record(result)?;
        println!();
        if let Some(shooter) = result.shooter() {
            println!("{shooter} shoots the moon!");
        }
        let points = result.points();
        println!(
            "Round {number}: N {} | E {} | you {} | W {}",
            points[0], points[1], points[2], points[3],
        );
        println!("Score — {}", score_line(game.scores()));
    }

    let settled = game.final_score().expect("the game just ended");
    let winners: Vec<Seat> = settled.winners().collect();
    println!();
    if winners == [HUMAN] {
        println!("You win the game! {}", score_line(settled.totals));
    } else if winners.contains(&HUMAN) {
        println!("You share the game! {}", score_line(settled.totals));
    } else {
        let names = winners
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" and ");
        println!("{names} wins the game. {}", score_line(settled.totals));
    }
    Ok(())
}

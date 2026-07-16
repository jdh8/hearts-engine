//! Cross-engine tournament: `MonteCarloBot` vs the brianberns/Hearts Deep
//! CFR model, bridged through `tournament/CfrShim` (see the README there).
//!
//! ```console
//! cargo run --release --example vs_cfr -- --deals 4 --throttle-ms 500
//! cargo run --release --example vs_cfr -- --deals 10000 --throttle-ms 0 \
//!     --shim "dotnet tournament/CfrShim/bin/Release/net10.0/CfrShim.dll http://localhost:8080"
//! ```
//!
//! Mirrors Brian's own benchmark (`Hearts/Tournament.fs`): duplicate 2v2
//! deals — each deal is replayed with the sides swapped between seat pairs
//! {East, West} and {North, South} — scored by the per-deal zero-sum payoff
//! `mean(others' points) − own points`.  The headline number is the CFR
//! side's mean payoff per seat-deal with a paired standard error; positive
//! means Deep CFR beats the Monte Carlo bot.
//!
//! Every shim reply carries Brian's legal-action set, which is checked
//! against ours on every decision, so any rules drift between the two
//! engines aborts the run instead of skewing it.

use anyhow::{Context as _, Result, bail, ensure};
use hearts_engine::hearts::{Card, PassDirection, Rules};
use hearts_engine::{MonteCarloBot, Strategy, Table, View};
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// Two-character card code shared with the shim, e.g. `QS`.
fn code(card: Card) -> String {
    format!("{}{}", card.rank.letter(), card.suit.letter())
}

/// A seat of the remote Deep CFR model behind a `CfrShim` subprocess.
struct CfrBot {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    throttle: Duration,
}

impl CfrBot {
    fn spawn(shim: &[String], throttle: Duration) -> Result<Self> {
        let mut child = Command::new(&shim[0])
            .args(&shim[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn shim {shim:?}"))?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = BufReader::new(child.stdout.take().expect("piped"));
        Ok(Self {
            child,
            stdin,
            stdout,
            throttle,
        })
    }

    /// One request/response cycle, cross-checking Brian's legal actions
    /// against ours.
    fn request(&mut self, body: &str, mut expected_legal: Vec<Card>) -> Result<Card> {
        std::thread::sleep(self.throttle);
        writeln!(self.stdin, "{body}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            bail!("shim exited early (request was {body})");
        }

        // {"card":"KC","legal":["5C","6C","KC"]} — we emit this shape
        // ourselves in Program.fs, so a two-token scan is enough.
        let card: Card = field(&line, "\"card\":\"", '"')?
            .parse()
            .ok()
            .with_context(|| format!("bad card in shim reply {line}"))?;
        let legal = field(&line, "\"legal\":[", ']')?;
        let mut legal: Vec<Card> = legal
            .split(',')
            .map(|s| s.trim_matches('"').parse().ok())
            .collect::<Option<_>>()
            .with_context(|| format!("bad legal set in shim reply {line}"))?;
        let key = |c: &Card| (c.suit as u8, c.rank.get());
        legal.sort_unstable_by_key(key);
        expected_legal.sort_unstable_by_key(key);
        ensure!(
            legal == expected_legal,
            "rules drift: their legal set {legal:?} vs ours {expected_legal:?} for {body}",
        );
        Ok(card)
    }
}

/// The text after `prefix` up to the closing `end`.
fn field<'a>(line: &'a str, prefix: &str, end: char) -> Result<&'a str> {
    let start = line
        .find(prefix)
        .with_context(|| format!("missing {prefix} in shim reply {line}"))?
        + prefix.len();
    let len = line[start..]
        .find(end)
        .with_context(|| format!("unterminated field in shim reply {line}"))?;
    Ok(&line[start..start + len])
}

fn cards_json(cards: impl IntoIterator<Item = Card>) -> String {
    let quoted: Vec<_> = cards
        .into_iter()
        .map(|c| format!("\"{}\"", code(c)))
        .collect();
    format!("[{}]", quoted.join(","))
}

fn direction_json(direction: PassDirection) -> &'static str {
    match direction {
        PassDirection::Left => "left",
        PassDirection::Right => "right",
        PassDirection::Across => "across",
        PassDirection::Hold => "hold",
    }
}

impl CfrBot {
    fn body(view: &View<'_>, kind: &str, outgoing: &[Card]) -> String {
        let mut plays = String::from("[");
        let current = view.current_trick();
        let tricks = view.tricks().iter().copied().chain(current);
        for (seat, card) in tricks.flat_map(|trick| trick.plays()) {
            let _ = write!(plays, "[\"{}\",\"{}\"],", seat.letter(), code(card));
        }
        if plays.ends_with(',') {
            plays.pop();
        }
        plays.push(']');
        format!(
            "{{\"kind\":\"{kind}\",\"seat\":\"{}\",\"dir\":\"{}\",\"hand\":{},\
             \"outgoing\":{},\"incoming\":{},\"plays\":{}}}",
            view.seat().letter(),
            direction_json(view.direction()),
            cards_json(view.hand()),
            cards_json(outgoing.iter().copied()),
            cards_json(view.received().into_iter().flatten()),
            plays,
        )
    }
}

impl Strategy for CfrBot {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        // His model passes one card per action; grow the outgoing set.
        let mut outgoing = Vec::with_capacity(3);
        for _ in 0..3 {
            let body = Self::body(view, "pass", &outgoing);
            let legal = view
                .hand()
                .into_iter()
                .filter(|card| !outgoing.contains(card))
                .collect();
            let card = self
                .request(&body, legal)
                .expect("CFR shim failed during pass");
            outgoing.push(card);
        }
        [outgoing[0], outgoing[1], outgoing[2]]
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        let outgoing: Vec<Card> = view.passed().into_iter().flatten().collect();
        let body = Self::body(view, "play", &outgoing);
        let legal = view.legal_plays().into_iter().collect();
        self.request(&body, legal)
            .expect("CFR shim failed during play")
    }

    fn name(&self) -> &str {
        "deep-cfr"
    }
}

impl Drop for CfrBot {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Config {
    deals: u32,
    seed: u64,
    samples: u32,
    throttle: Duration,
    shim: Vec<String>,
}

fn usage() {
    println!(
        "Usage: vs_cfr [--deals N] [--seed N] [--samples N] [--throttle-ms N] [--shim CMD]\n\
         N deals must be even (each deal is replayed with sides swapped).\n\
         CMD defaults to \"dotnet tournament/CfrShim/bin/Release/net10.0/CfrShim.dll\";\n\
         append an endpoint URL to CMD to use a local Hearts.Web harness."
    );
}

fn parse_args() -> Result<Option<Config>> {
    let shim_dll = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tournament/CfrShim/bin/Release/net10.0/CfrShim.dll"
    );
    let mut config = Config {
        deals: 4,
        seed: 1,
        samples: 128,
        throttle: Duration::from_millis(500),
        shim: vec!["dotnet".into(), shim_dll.into()],
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--deals" => config.deals = value()?.parse()?,
            "--seed" => config.seed = value()?.parse()?,
            "--samples" => config.samples = value()?.parse()?,
            "--throttle-ms" => config.throttle = Duration::from_millis(value()?.parse()?),
            "--shim" => config.shim = value()?.split_whitespace().map(String::from).collect(),
            "--help" | "-h" => return Ok(None),
            other => bail!("unknown flag {other:?}"),
        }
    }
    ensure!(
        config.deals.is_multiple_of(2) && config.deals > 0,
        "--deals must be positive and even"
    );
    ensure!(!config.shim.is_empty(), "--shim must not be empty");
    Ok(Some(config))
}

/// Brian's zero-sum payoff: mean of the other players' points minus own.
fn payoffs(scores: [u16; 4]) -> [f64; 4] {
    let sum: u16 = scores.iter().sum();
    scores.map(|own| f64::from(sum - own) / 3.0 - f64::from(own))
}

fn main() -> Result<()> {
    let Some(config) = parse_args()? else {
        usage();
        return Ok(());
    };
    let rules = Rules::new();
    let mut seed_rng = StdRng::seed_from_u64(config.seed);
    let mut mc: [MonteCarloBot<StdRng>; 2] = std::array::from_fn(|_| {
        MonteCarloBot::new(StdRng::from_rng(&mut seed_rng)).samples(config.samples)
    });
    let [mc0, mc1] = &mut mc;
    let mut cfr0 = CfrBot::spawn(&config.shim, config.throttle)?;
    let mut cfr1 = CfrBot::spawn(&config.shim, config.throttle)?;

    let pairs = config.deals / 2;
    let mut pair_payoffs = Vec::with_capacity(pairs as usize);
    let mut points = [0u64; 2]; // [mc, cfr], moon-adjusted like Brian's
    let mut moons = [0u32; 2];
    let start = Instant::now();

    for pair in 0..pairs {
        let direction = PassDirection::from_deal_index(pair);
        let deal_seed = config.seed ^ u64::from(pair).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut payoff_sum = 0.0;

        // Seating 0: CFR at East and West; seating 1: CFR at North and South.
        for seating in 0..2 {
            let mut rng = StdRng::seed_from_u64(deal_seed);
            let mut table = Table::deal(rules, direction, &mut rng);
            let strategies: [&mut dyn Strategy; 4] = if seating == 0 {
                [mc0, &mut cfr0, mc1, &mut cfr1]
            } else {
                [&mut cfr0, mc0, &mut cfr1, mc1]
            };
            let result = table.play(strategies)?;
            let scores = result.scores(&rules);
            let cfr_seats = if seating == 0 { [1, 3] } else { [0, 2] };
            let payoff = payoffs(scores);
            for (seat, &score) in scores.iter().enumerate() {
                let is_cfr = cfr_seats.contains(&seat);
                points[usize::from(is_cfr)] += u64::from(score);
                if is_cfr {
                    payoff_sum += payoff[seat];
                }
            }
            if let Some(shooter) = result.shooter() {
                let is_cfr = cfr_seats.contains(&(shooter as usize));
                moons[usize::from(is_cfr)] += 1;
            }
        }
        pair_payoffs.push(payoff_sum / 4.0);
        eprint!("\r{}/{pairs} deal pairs", pair + 1);
    }
    eprintln!();

    let n = f64::from(pairs);
    let mean = pair_payoffs.iter().sum::<f64>() / n;
    let var = pair_payoffs.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let se = (var / n).sqrt();
    let deal_count = f64::from(config.deals);

    println!(
        "{} deals ({pairs} duplicate pairs) in {:.1?}",
        config.deals,
        start.elapsed(),
    );
    println!(
        "deep-cfr payoff per seat-deal: {mean:+.3} ± {se:.3} (paired SE; positive favors CFR)"
    );
    println!(
        "penalty points per deal (moon-adjusted): deep-cfr {:.2}, mc:{} {:.2}",
        points[1] as f64 / deal_count,
        config.samples,
        points[0] as f64 / deal_count,
    );
    println!("moons: mc {} / cfr {}", moons[0], moons[1]);
    Ok(())
}

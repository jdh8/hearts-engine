//! How long a Hearts bot takes to decide.
//!
//! ```console
//! cargo bench
//! ```

use criterion::{Criterion, criterion_group, criterion_main};
use hearts_engine::hearts::{PassDirection, Phase, Round, Rules, Seat};
use hearts_engine::{HeuristicBot, MonteCarloBot, Strategy, Table, play_round};
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use std::hint::black_box;

fn fixed_deal() -> Round {
    let mut rng = StdRng::seed_from_u64(7);
    Round::deal(Rules::new(), PassDirection::Hold, &mut rng)
}

/// A table paused on a stable mid-round play with more than one legal card.
fn play_position() -> (Table, Seat) {
    let mut table = Table::new(fixed_deal());
    let mut bot = HeuristicBot::new();
    loop {
        let seat = table.turn().expect("a mid-round position exists");
        if table.round().phase() == Phase::Playing
            && table.round().tricks().len() >= 4
            && table.view(seat).legal_plays().len() > 1
        {
            return (table, seat);
        }
        table.step(&mut bot).expect("greedy play is legal");
    }
}

/// A stable deal waiting for North's three-card pass.
fn pass_position() -> Table {
    let mut rng = StdRng::seed_from_u64(11);
    Table::deal(Rules::new(), PassDirection::Left, &mut rng)
}

fn decisions(c: &mut Criterion) {
    let (table, seat) = play_position();
    let pass_table = pass_position();
    let passer = pass_table.turn().expect("passing starts at North");

    c.bench_function("heuristic play", |b| {
        let mut bot = HeuristicBot::new();
        b.iter(|| black_box(bot.play_card(&table.view(seat))));
    });

    c.bench_function("greedy round", |b| {
        b.iter(|| {
            play_round(
                black_box(fixed_deal()),
                [
                    &mut HeuristicBot::new(),
                    &mut HeuristicBot::new(),
                    &mut HeuristicBot::new(),
                    &mut HeuristicBot::new(),
                ],
            )
            .expect("legal play")
        });
    });

    for samples in [16, 64, 128] {
        c.bench_function(&format!("monte carlo play, {samples} samples"), |b| {
            let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1)).samples(samples);
            b.iter(|| black_box(bot.play_card(&table.view(seat))));
        });
    }

    c.bench_function("monte carlo pass, 64 samples", |b| {
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1)).samples(64);
        b.iter(|| black_box(bot.pass_cards(&pass_table.view(passer))));
    });

    // An Expert-sized decision is slow enough that criterion's default 100
    // measurements would take minutes; 10 keeps the arm honest and quick.
    let mut group = c.benchmark_group("expert");
    group.sample_size(10);
    group.bench_function("monte carlo play, 1024 samples", |b| {
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1)).samples(1024);
        b.iter(|| black_box(bot.play_card(&table.view(seat))));
    });
    group.finish();
}

criterion_group!(benches, decisions);
criterion_main!(benches);

// Thin renderer over the wasm engine, paced so each move is a separate,
// animated step. The engine runs the game synchronously; while it is the
// bots' turn (or a forced human step) JS ticks it on a timer and flies one
// card per step between a player's fan and the four-seat trick.
import init, { WebGame } from './pkg/hearts_web.js';

const PACE_MS = 650; // pause between bot steps, so the table can be followed
const FLY_MS = 350; // card glide duration — keep in sync with `.ghost`
const HINT_SAMPLES = 128; // == the Expert bot; more sampled worlds show no measurable gain

const SUITS = {
  C: ['♣', 'green'],
  D: ['♦', 'blue'],
  H: ['♥', 'red'],
  S: ['♠', 'black'],
};
const GLYPH_TO_SUIT = { '♣': 'C', '♦': 'D', '♥': 'H', '♠': 'S' };
const SUIT_ORDER = { C: 0, D: 1, H: 2, S: 3 };
const RANKS = { 2: '2', 3: '3', 4: '4', 5: '5', 6: '6', 7: '7', 8: '8', 9: '9', 10: '10', 11: 'J', 12: 'Q', 13: 'K', 14: 'A' };
const RANK_VALUES = { T: 10, J: 11, Q: 12, K: 13, A: 14 };
const NAMES = ['You', 'West', 'North', 'East'];

let game;
let state; // snapshot currently on screen (the "before" state during a step)
let busy = false;
let selectedPass = new Set();

const id = (x) => document.getElementById(x);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// One-shot synthesized stings; no assets, no game-loop impact on failure.
let audioCtx;
function chime(notes) {
  try {
    audioCtx ??= new AudioContext();
    audioCtx.resume();
    const t0 = audioCtx.currentTime;
    for (const { freq, type, at, dur, peak = 0.15 } of notes) {
      const gain = new GainNode(audioCtx, { gain: 0 });
      gain.gain.setValueAtTime(0, t0 + at);
      gain.gain.linearRampToValueAtTime(peak, t0 + at + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.001, t0 + at + dur);
      gain.connect(audioCtx.destination);
      let src;
      if (type === 'noise') {
        // White noise highpassed at `freq`, for percussive cracks.
        const length = Math.ceil(audioCtx.sampleRate * dur);
        const buffer = new AudioBuffer({ length, sampleRate: audioCtx.sampleRate });
        const data = buffer.getChannelData(0);
        for (let i = 0; i < length; i++) data[i] = Math.random() * 2 - 1;
        src = new AudioBufferSourceNode(audioCtx, { buffer });
        src
          .connect(new BiquadFilterNode(audioCtx, { type: 'highpass', frequency: freq }))
          .connect(gain);
      } else {
        src = new OscillatorNode(audioCtx, { type, frequency: freq });
        src.connect(gain);
      }
      src.start(t0 + at);
      src.stop(t0 + at + dur);
    }
  } catch {
    // audio is best-effort
  }
}

// Breaking glass: a noise crack plus inharmonic partials ringing out in the
// 2–7 kHz band, staggered like shards scattering.
const heartsBrokenSound = () =>
  chime([
    { freq: 3000, type: 'noise', at: 0, dur: 0.08, peak: 0.3 },
    { freq: 2093, type: 'sine', at: 0.01, dur: 0.3, peak: 0.12 },
    { freq: 2960, type: 'sine', at: 0.02, dur: 0.25, peak: 0.1 },
    { freq: 4730, type: 'sine', at: 0.015, dur: 0.2, peak: 0.08 },
    { freq: 6260, type: 'sine', at: 0.035, dur: 0.15, peak: 0.06 },
  ]);
// Ominous low tritone dyad around C3.  Sawtooth + boosted peak: near 130 Hz
// the ear needs ~20 dB more SPL for equal loudness, and small speakers barely
// reproduce the fundamental, so let the harmonics carry it.
const queenSound = () =>
  chime([
    { freq: 130.81, type: 'sawtooth', at: 0, dur: 0.4, peak: 0.35 },
    { freq: 185.0, type: 'sawtooth', at: 0, dur: 0.4, peak: 0.35 },
  ]);

async function main() {
  await init();

  const savedDifficulty = localStorage.getItem('difficulty');
  if (savedDifficulty) id('difficulty').value = savedDifficulty;
  id('difficulty').onchange = () => {
    localStorage.setItem('difficulty', id('difficulty').value);
  };

  const savedLog = localStorage.getItem('log-visible') === 'true';
  document.body.classList.toggle('log-hidden', !savedLog);
  id('logtoggle').onclick = toggleLog;
  id('hint-button').onclick = showHint;
  updateLogButton();

  document.addEventListener('keydown', (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey || event.repeat) return;
    if (/^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName)) return;
    if (event.key.toLowerCase() === 'h') showHint();
    if (event.key.toLowerCase() === 'l') toggleLog();
    if (event.key.toLowerCase() === 'n') {
      if (state?.round_over) continueGame();
      else newGame();
    }
  });

  await newGame();
}

function toggleLog() {
  document.body.classList.toggle('log-hidden');
  localStorage.setItem('log-visible', String(!document.body.classList.contains('log-hidden')));
  updateLogButton();
}

function updateLogButton() {
  id('logtoggle').textContent =
    document.body.classList.contains('log-hidden') ? 'Show log' : 'Hide log';
}

function setBusy(value) {
  busy = value;
  document.body.classList.toggle('busy', value);
  id('hint-button').disabled = value || !state?.your_turn;
  id('confirm-pass')?.toggleAttribute('disabled', value || selectedPass.size !== 3);
}

async function newGame() {
  if (busy) return;
  setBusy(true);
  selectedPass.clear();
  hideHint();
  const seed = String(Math.floor(Math.random() * 2 ** 53));
  game = new WebGame(id('difficulty').value, '', seed);
  state = JSON.parse(game.snapshot());
  render(state);
  await run();
  setBusy(false);
}

// Apply a human decision, then pace the bots' replies.
async function act(method, ...args) {
  if (busy) return;
  setBusy(true);
  await step(JSON.parse(game[method](...args)));
  await run();
  setBusy(false);
}

// Tick at most one visible engine action at a time.
async function run() {
  while (state && !state.your_turn && !state.game_over && !state.round_over) {
    await delay(PACE_MS);
    await step(JSON.parse(game.tick()));
  }
}

async function continueGame() {
  if (busy || !state?.round_over) return;
  setBusy(true);
  selectedPass.clear();
  state = JSON.parse(game.next_deal());
  render(state);
  await run();
  setBusy(false);
}

// End a round whose points are already settled: jump straight to the
// showdown, skipping the scoreless run-out (no per-card animation).
async function finishRound() {
  if (busy || !state?.points_settled) return;
  setBusy(true);
  state = JSON.parse(game.finish_round());
  render(state);
  setBusy(false);
}

// Animate the move that produced `next` over the current view, then render it.
// A fourth play is briefly rendered as a complete trick before all four cards
// sweep toward its winner.
async function step(next) {
  const move = next.last_move;
  if (move?.kind === 'play' && move.card) {
    if (move.card === 'Q♠') queenSound();
    else if (!state?.hearts_broken && next.hearts_broken) heartsBrokenSound();
    const from = actorAnchor(move.actor, move.card);
    await flyCard(from, id(`trick-slot-${move.actor}`), cardFromCode(move.card));
  } else if (move?.kind === 'pass') {
    await flyPass(actorAnchor(move.actor), id('pass-pot'));
  } else if (move?.kind === 'exchange') {
    await flyExchange();
  }

  const trickCompleted =
    move?.kind === 'play' &&
    state?.trick.some(Boolean) &&
    next.trick.every((card) => !card) &&
    next.last_trick.some(Boolean);

  if (trickCompleted) {
    render(next, next.last_trick, next.last_trick_winner);
    await delay(Math.round(PACE_MS * 0.55));
    await sweepTrick(next.last_trick_winner);
  }

  render(next);
  state = next;
}

function actorAnchor(seat, code = '') {
  if (seat === 0 && code) {
    return id('hand').querySelector(`[data-code="${cssEscape(code)}"]`) || id('hand');
  }
  return seat === 0 ? id('hand') : id(`seat-${seat}`).querySelector('.fan') || id(`seat-${seat}`);
}

async function flyPass(from, to) {
  await Promise.all([0, 70, 140].map(async (wait) => {
    await delay(wait);
    await flyCard(from, to, null);
  }));
}

async function flyExchange() {
  const pot = id('pass-pot');
  await Promise.all([0, 1, 2, 3].map(async (seat) => {
    await delay(seat * 55);
    await flyCard(pot, actorAnchor(seat), null);
  }));
}

function flyCard(fromEl, toEl, face) {
  return new Promise((resolve) => {
    if (!fromEl || !toEl) return resolve();
    const from = fromEl.getBoundingClientRect();
    const to = toEl.getBoundingClientRect();
    const ghost = face ? cardEl(face) : backEl();
    ghost.classList.add('ghost');
    ghost.style.left = `${from.left + from.width / 2 - cardWidth() / 2}px`;
    ghost.style.top = `${from.top + from.height / 2 - cardHeight() / 2}px`;
    document.body.appendChild(ghost);
    const dx = to.left + to.width / 2 - (from.left + from.width / 2);
    const dy = to.top + to.height / 2 - (from.top + from.height / 2);
    requestAnimationFrame(() => {
      ghost.style.transform = `translate(${dx}px, ${dy}px)`;
    });
    finishTransition(ghost, resolve);
  });
}

async function sweepTrick(winner) {
  if (winner == null) return;
  const target = actorAnchor(winner);
  const to = target.getBoundingClientRect();
  const cards = [...id('trick').querySelectorAll('.trick-slot .card')];
  await Promise.all(cards.map((card, index) => new Promise((resolve) => {
    const from = card.getBoundingClientRect();
    const ghost = card.cloneNode(true);
    ghost.classList.add('ghost', 'sweeping');
    ghost.style.left = `${from.left}px`;
    ghost.style.top = `${from.top}px`;
    document.body.appendChild(ghost);
    requestAnimationFrame(() => {
      const dx = to.left + to.width / 2 - (from.left + from.width / 2);
      const dy = to.top + to.height / 2 - (from.top + from.height / 2);
      ghost.style.transitionDelay = `${index * 35}ms`;
      ghost.style.transform = `translate(${dx}px, ${dy}px) scale(.65)`;
      ghost.style.opacity = '0';
    });
    finishTransition(ghost, resolve, index * 35);
  })));
}

function finishTransition(el, resolve, extra = 0) {
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    el.remove();
    resolve();
  };
  el.addEventListener('transitionend', finish, { once: true });
  setTimeout(finish, FLY_MS + extra + 180);
}

function cardWidth() {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--card-w')) *
    parseFloat(getComputedStyle(document.documentElement).fontSize);
}

function cardHeight() {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--card-h')) *
    parseFloat(getComputedStyle(document.documentElement).fontSize);
}

// --- rendering -------------------------------------------------------------

function render(snapshot, trickOverride = null, winnerOverride = null) {
  renderHeader(snapshot);
  renderOpponents(snapshot);
  renderTrick(trickOverride || snapshot.trick, trickOverride ? winnerOverride : snapshot.trick_winner, snapshot);
  renderHand(snapshot);
  renderActions(snapshot);
  renderLog(snapshot);
  renderShowdown(snapshot);
}

function renderHeader(snapshot) {
  id('score').textContent = NAMES.map((name, seat) => `${name} ${snapshot.scores[seat]}`).join(' · ');
  const broken = id('broken');
  broken.classList.toggle('lit', snapshot.hearts_broken);
  broken.title = snapshot.hearts_broken ? 'Hearts are broken' : 'Hearts are not broken';
  broken.setAttribute('aria-label', broken.title);
  id('hint-button').disabled = busy || !snapshot.your_turn;
}

function renderOpponents(snapshot) {
  snapshot.opponents.forEach((opponent, index) => {
    const seat = index + 1;
    const zone = id(`seat-${seat}`);
    zone.innerHTML = '';
    const label = document.createElement('div');
    label.className = 'seat-label';
    label.append(
      text(opponent.name, 'seat-name'),
      text(`${opponent.tricks} trick${opponent.tricks === 1 ? '' : 's'}`, 'trick-count'),
      text(`${opponent.points} pts`, 'seat-badge'),
    );
    const fan = document.createElement('div');
    fan.className = 'fan';
    for (let card = 0; card < opponent.hand_len; card++) fan.appendChild(backEl());
    zone.append(label, fan);
  });
}

function renderTrick(cards, winner, snapshot) {
  cards.forEach((code, seat) => {
    const slot = id(`trick-slot-${seat}`);
    slot.innerHTML = '';
    slot.classList.toggle('winning', code != null && seat === winner);
    slot.appendChild(code ? cardEl(cardFromCode(code)) : slotEl());
  });
  const passPot = id('pass-pot');
  passPot.hidden = snapshot.phase !== 'passing';
  id('trick').classList.toggle('passing', snapshot.phase === 'passing');
}

function renderHand(snapshot) {
  const hand = id('hand');
  hand.innerHTML = '';
  const firstTrick = completedTricks(snapshot) === 0;
  const passing = snapshot.your_turn && snapshot.phase === 'passing';
  const cards = [...snapshot.hand].sort(
    (a, b) => SUIT_ORDER[a.suit] - SUIT_ORDER[b.suit] || a.rank - b.rank,
  );

  cards.forEach((card) => {
    const el = cardEl(card);
    el.dataset.code = card.code;
    if (firstTrick && card.received) el.classList.add('received');
    if (passing) {
      el.disabled = false;
      el.classList.add('clickable', 'passable');
      el.classList.toggle('selected', selectedPass.has(card.code));
      el.setAttribute('aria-pressed', String(selectedPass.has(card.code)));
      el.onclick = () => togglePass(card.code, el);
    } else if (snapshot.phase === 'playing') {
      el.classList.toggle('clickable', snapshot.your_turn && card.legal);
      el.classList.toggle('illegal', !card.legal);
      if (snapshot.your_turn && card.legal) {
        el.disabled = false;
        el.onclick = () => act('play', card.code);
      }
    }
    hand.appendChild(el);
  });
  id('you-points').textContent = `${snapshot.round_points[0]} pts`;
}

function completedTricks(snapshot) {
  const lengths = [snapshot.hand.length, ...snapshot.opponents.map((opponent) => opponent.hand_len)];
  return Math.max(0, 13 - Math.max(...lengths));
}

function togglePass(code, el) {
  if (busy) return;
  if (selectedPass.has(code)) selectedPass.delete(code);
  else if (selectedPass.size < 3) selectedPass.add(code);
  else return;
  el.classList.toggle('selected', selectedPass.has(code));
  el.setAttribute('aria-pressed', String(selectedPass.has(code)));
  const confirm = id('confirm-pass');
  confirm.disabled = selectedPass.size !== 3;
  id('pass-count').textContent = `${selectedPass.size} of 3 selected`;
}

function renderActions(snapshot) {
  const box = id('actions');
  box.innerHTML = '';
  hideHint(); // a solver read belongs to one decision only

  if (snapshot.game_over) {
    box.append(text('Game over', 'banner'));
    return;
  }
  if (snapshot.round_over) {
    box.append(text(`Round ${snapshot.round_no} complete`, 'banner'));
    return;
  }
  if (snapshot.phase === 'passing') {
    if (!snapshot.your_turn) {
      box.append(text('The other players are choosing their passes…', 'muted'));
      return;
    }
    const direction = snapshot.pass_direction.toLowerCase();
    box.append(
      text(`Pass three cards ${direction === 'across' ? 'across' : `to the ${direction}`}.`, 'prompt'),
      text(`${selectedPass.size} of 3 selected`, 'muted', 'pass-count'),
      button('Confirm', confirmPass, 'confirm-pass'),
    );
    id('confirm-pass').disabled = selectedPass.size !== 3;
    return;
  }
  if (!snapshot.your_turn) {
    box.append(text(snapshot.phase === 'finished' ? 'Counting the round…' : 'An opponent is thinking…', 'muted'));
    return;
  }
  box.append(text('Choose a highlighted card to play.', 'prompt'));
  if (snapshot.points_settled) {
    box.append(button('End round', finishRound, 'end-round'));
  }
}

function confirmPass() {
  if (selectedPass.size !== 3) return;
  const codes = [...selectedPass].join(' ');
  selectedPass.clear();
  act('pass_cards', codes);
}

function renderShowdown(snapshot) {
  const panel = id('showdown');
  if (!snapshot.round_over && !snapshot.game_over) {
    panel.hidden = true;
    panel.innerHTML = '';
    return;
  }

  const result = snapshot.result || [0, 0, 0, 0];
  const moon = snapshot.moon == null
    ? ''
    : `<div class="moon">${escapeHtml(NAMES[snapshot.moon])} shot the moon!</div>`;
  const rows = snapshot.score_sheet.map((totals, index) =>
    `<tr><th scope="row">${index + 1}</th>${totals.map((value) => `<td>${value}</td>`).join('')}</tr>`,
  ).join('');
  const winner = snapshot.game_over
    ? `<div class="winner">${escapeHtml(winnerLine(snapshot.winners))}</div>`
    : `<h2>Round ${snapshot.round_no}</h2>`;
  const action = snapshot.game_over ? 'New game' : 'Continue';
  panel.innerHTML =
    `<div class="showdown-sheet">${winner}${moon}` +
    `<div class="round-result">${NAMES.map((name, seat) => `<span><b>${name}</b>${result[seat]} pts</span>`).join('')}</div>` +
    '<table><caption>Cumulative scores</caption><thead><tr><th>Round</th>' +
    NAMES.map((name) => `<th scope="col">${name}</th>`).join('') +
    `</tr></thead><tbody>${rows}</tbody></table>` +
    `<button id="showdown-action">${action}</button></div>`;
  id('showdown-action').onclick = snapshot.game_over ? newGame : continueGame;
  panel.hidden = false;
}

function winnerLine(winners) {
  if (winners.length === 1) return winners[0] === 'You' ? 'You win!' : `${winners[0]} wins.`;
  return `${winners.join(' and ')} tie.`;
}

// --- hint ------------------------------------------------------------------

function showHint() {
  if (busy || !state?.your_turn) return;
  const rows = JSON.parse(game.hint(HINT_SAMPLES));
  if (!rows.length) return hideHint();
  renderHint(rows);
}

function hideHint() {
  const panel = id('hint');
  panel.hidden = true;
  panel.innerHTML = '';
}

function renderHint(rows) {
  const body = rows.map((row) =>
    `<div class="hint-row${row.recommended ? ' best' : ''}">` +
      `<span>${escapeHtml(row.action)}</span>` +
      `<span>${(row.equity * 100).toFixed(1)}%</span>` +
      `<span>${row.ev.toFixed(1)}</span></div>`,
  ).join('');
  const panel = id('hint');
  panel.innerHTML =
    '<h2>Solver</h2>' +
    '<p class="hint-note">Equity is your chance to win. Round EV is expected penalty points; lower is better.</p>' +
    '<div class="hint-row hint-head"><span>Move</span><span>Equity</span><span>Round EV ↓</span></div>' +
    body;
  panel.hidden = false;
}

function renderLog(snapshot) {
  const log = id('log');
  log.innerHTML = '<h2>Log</h2>' + snapshot.log.map((line) => `<div>${escapeHtml(line)}</div>`).join('');
  log.scrollTop = log.scrollHeight;
}

// --- element helpers -------------------------------------------------------

function cardEl(card) {
  const [glyph, colour] = SUITS[card.suit];
  const el = document.createElement('button');
  el.type = 'button';
  el.className = `card ${colour}`;
  el.innerHTML = `<span class="rank">${RANKS[card.rank]}</span><span class="suit">${glyph}</span>`;
  el.setAttribute('aria-label', card.code || `${RANKS[card.rank]}${glyph}`);
  el.disabled = true;
  return el;
}

function cardFromCode(code) {
  const last = code.slice(-1);
  const suit = GLYPH_TO_SUIT[last] || last.toUpperCase();
  const rankText = code.slice(0, -1).toUpperCase();
  const rank = Number(rankText) || RANK_VALUES[rankText] || 10;
  return { code, suit, rank };
}

function backEl() {
  const el = document.createElement('div');
  el.className = 'card back';
  el.setAttribute('aria-hidden', 'true');
  return el;
}

function slotEl() {
  const el = document.createElement('div');
  el.className = 'card slot';
  return el;
}

function button(label, onclick, elementId = '') {
  const element = document.createElement('button');
  element.textContent = label;
  element.onclick = onclick;
  if (elementId) element.id = elementId;
  return element;
}

function text(value, className = '', elementId = '') {
  const element = document.createElement('span');
  element.textContent = value;
  if (className) element.className = className;
  if (elementId) element.id = elementId;
  return element;
}

function escapeHtml(value) {
  const element = document.createElement('div');
  element.textContent = value;
  return element.innerHTML;
}

function cssEscape(value) {
  return window.CSS?.escape ? CSS.escape(value) : value.replace(/["\\]/g, '\\$&');
}

main();

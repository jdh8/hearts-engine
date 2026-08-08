// Thin renderer over the wasm engine, paced so each move is a separate,
// animated step. The engine runs the game synchronously; while it is the
// bots' turn (or a forced human step) JS ticks it on a timer and flies one
// card per step between a player's fan and the four-seat trick.
import init, { WebGame } from './pkg/hearts_web.js';

const PACE_MS = 650; // pause between bot steps, so the table can be followed
const FLY_MS = 350; // card glide duration — keep in sync with `.ghost`
const HINT_SAMPLES = 256; // match the Expert bot; 128 and 256 disagree often enough to matter

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
const PASS_RECIPIENTS = { left: 'West', across: 'North', right: 'East' };

let game;
let state; // snapshot currently on screen (the "before" state during a step)
let busy = false;
let epoch = 0; // bumped when a click outruns an in-flight animation
let selectedPass = new Set();
let moonEffectRound = null; // a showdown may render repeatedly; cue it once

const id = (x) => document.getElementById(x);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// One-shot stings; audio is best-effort and never blocks the game loop.
let muted = localStorage.getItem('muted') === 'true';
const glassBreak = new Audio(new URL('./audio/glass-break.mp3', import.meta.url));
const queenKnell = new Audio(new URL('./audio/queen-knell.mp3', import.meta.url));
const moonFireworks = new Audio(new URL('./audio/moon-fireworks.mp3', import.meta.url));
const moonGunshot = new Audio(new URL('./audio/moon-gunshot.mp3', import.meta.url));
glassBreak.preload = 'auto';
glassBreak.volume = 0.6;
queenKnell.preload = 'auto';
queenKnell.volume = 0.35;
moonFireworks.preload = 'auto';
moonFireworks.volume = 0.55;
moonGunshot.preload = 'auto';
moonGunshot.volume = 0.2;

function playSound(sound) {
  if (muted) return;
  try {
    sound.currentTime = 0;
    sound.play().catch(() => {});
  } catch {
    // audio is best-effort
  }
}

const heartsBrokenSound = () => playSound(glassBreak);
const queenSound = () => playSound(queenKnell);

// Visual counterpart to the stings, so the events land without audio.
function flashSting(glyph, cls) {
  const el = document.createElement('div');
  el.className = `sting ${cls}`;
  el.textContent = glyph;
  el.setAttribute('aria-hidden', 'true'); // already announced via the log
  document.body.appendChild(el);
  el.addEventListener('animationend', () => el.remove());
  setTimeout(() => el.remove(), 1500); // fallback
}

async function main() {
  await init();

  const oldDifficulty = localStorage.getItem('difficulty');
  const migratedDifficulty = {
    newbie: 'newbie',
    'mc:16': 'mc:32',
    'mc:64': 'mc:128',
    'mc:128': 'mc:256',
  }[oldDifficulty];
  const savedDifficulty = localStorage.getItem('difficulty-v2') || migratedDifficulty;
  if (savedDifficulty) id('difficulty').value = savedDifficulty;
  id('difficulty').onchange = () => {
    localStorage.setItem('difficulty-v2', id('difficulty').value);
  };

  const savedLog = localStorage.getItem('log-visible') === 'true';
  document.body.classList.toggle('log-hidden', !savedLog);
  id('logtoggle').onclick = toggleLog;
  id('hint-button').onclick = showHint;
  id('mute').onclick = toggleMute;
  id('end-round').onclick = finishRound;
  updateLogButton();
  updateMuteButton();

  document.addEventListener('keydown', (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey || event.repeat) return;
    if (/^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName)) return;
    if (event.key.toLowerCase() === 'h') showHint();
    if (event.key.toLowerCase() === 'l') toggleLog();
    if (event.key.toLowerCase() === 'm') toggleMute();
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

function toggleMute() {
  muted = !muted;
  localStorage.setItem('muted', String(muted));
  updateMuteButton();
}

function updateMuteButton() {
  const el = id('mute');
  el.textContent = muted ? '🔇' : '🔊';
  el.title = muted ? 'Unmute sounds (m)' : 'Mute sounds (m)';
  el.setAttribute('aria-label', el.title);
  el.setAttribute('aria-pressed', String(muted));
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
  moonEffectRound = null;
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
// Deliberately not gated on `busy` — the offer stands while the bots play,
// so this is exactly the state it has to work in.  `run()` unwinds on its
// own once `state.round_over` holds, and its in-flight `tick()` is a no-op.
function finishRound() {
  if (!state?.points_settled) return;
  epoch++;
  render((state = JSON.parse(game.finish_round())));
}

// Animate the move that produced `next` over the current view, then render it.
// A fourth play is briefly rendered as a complete trick before all four cards
// sweep toward its winner.
async function step(next) {
  const mine = epoch;
  const move = next.last_move;
  if (move?.kind === 'play' && move.card) {
    if (move.card === 'Q♠') {
      queenSound();
      flashSting('♠', 'spade');
    } else if (!state?.hearts_broken && next.hearts_broken) {
      heartsBrokenSound();
      flashSting('💔', 'heart');
    }
    const from = actorAnchor(move.actor, move.card);
    await flyCard(from, id(`trick-slot-${move.actor}`), cardFromCode(move.card));
  } else if (move?.kind === 'pass') {
    await flyPass(actorAnchor(move.actor), id('pass-pot'));
  } else if (move?.kind === 'exchange') {
    await flyExchange();
  }

  if (epoch !== mine) return; // an End round click landed mid-flight

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
  renderMoonFeedback(snapshot);
  // Outside `#actions`, which is wiped every frame: the offer has to survive
  // the bots' turns, so it lives in a node no render can destroy.
  id('end-round').hidden = !snapshot.points_settled;
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

  const passDirection = id('pass-direction');
  const direction = snapshot.pass_direction.toLowerCase();
  const recipient = PASS_RECIPIENTS[direction];
  const showDirection = snapshot.phase === 'passing' && recipient != null;
  passDirection.hidden = !showDirection;
  if (showDirection) {
    passDirection.dataset.direction = direction;
    passDirection.setAttribute('aria-label', `Pass to ${recipient}`);
  } else {
    delete passDirection.dataset.direction;
    passDirection.removeAttribute('aria-label');
  }
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
    : `<div class="moon ${snapshot.moon === 0 ? 'moon-player' : 'moon-opponent'}">` +
      `${escapeHtml(NAMES[snapshot.moon])} shot the moon!</div>`;
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

// A moon is present only on a terminal snapshot. The showdown can be rendered
// again while controls settle, so key the cue to the round rather than the
// number of renders. Visuals still fire while muted, like the card stings.
function renderMoonFeedback(snapshot) {
  if (
    snapshot.moon == null ||
    (!snapshot.round_over && !snapshot.game_over) ||
    moonEffectRound === snapshot.round_no
  ) return;

  moonEffectRound = snapshot.round_no;
  const playerShot = snapshot.moon === 0;
  playSound(playerShot ? moonFireworks : moonGunshot);
  launchMoonBurst(playerShot);
}

function launchMoonBurst(playerShot) {
  const layer = document.createElement('div');
  layer.className = `moon-burst ${playerShot ? 'moon-burst-player' : 'moon-burst-opponent'}`;
  layer.setAttribute('aria-hidden', 'true'); // the showdown already announces the result

  if (playerShot) buildPlayerBurst(layer);
  else buildOpponentBurst(layer);

  id('showdown').prepend(layer);
  setTimeout(() => layer.remove(), 2400);
}

function buildPlayerBurst(layer) {
  const colours = ['#ff4d6d', '#ffd166', '#38bdf8', '#5ee38d', '#c77dff', '#ff8c42'];
  const centres = [[17, 30], [83, 31], [50, 13]];

  for (let index = 0; index < 45; index++) {
    const particle = document.createElement('i');
    const centre = centres[index % centres.length];
    const angle = (index * 137.5) % 360;
    const radians = angle * Math.PI / 180;
    const distance = 95 + (index % 6) * 22;
    particle.className = 'moon-particle moon-firework-particle';
    particle.style.setProperty('--x', `${centre[0]}%`);
    particle.style.setProperty('--y', `${centre[1]}%`);
    particle.style.setProperty('--dx', `${Math.cos(radians) * distance}px`);
    particle.style.setProperty('--dy', `${Math.sin(radians) * distance}px`);
    particle.style.setProperty('--spin', `${angle + 90}deg`);
    particle.style.setProperty('--particle-colour', colours[index % colours.length]);
    particle.style.setProperty('--delay', `${(index % 3) * 0.12}s`);
    layer.appendChild(particle);
  }

  for (let index = 0; index < 24; index++) {
    const confetti = document.createElement('i');
    confetti.className = 'moon-particle moon-confetti';
    confetti.style.setProperty('--x', `${2 + (index * 41) % 96}%`);
    confetti.style.setProperty('--drift', `${((index * 29) % 180) - 90}px`);
    confetti.style.setProperty('--spin', `${360 + (index % 5) * 120}deg`);
    confetti.style.setProperty('--particle-colour', colours[(index * 5) % colours.length]);
    confetti.style.setProperty('--delay', `${0.08 + (index % 7) * 0.05}s`);
    confetti.style.setProperty('--duration', `${1.45 + (index % 5) * 0.1}s`);
    layer.appendChild(confetti);
  }
}

function buildOpponentBurst(layer) {
  const flash = document.createElement('i');
  flash.className = 'moon-impact-flash';
  const ring = document.createElement('i');
  ring.className = 'moon-impact-ring';
  layer.append(flash, ring);

  const colours = ['#ff3b30', '#c81d17', '#721c18', '#f58a80', '#2b1717'];
  for (let index = 0; index < 30; index++) {
    const spark = document.createElement('i');
    const angle = (index * 47) % 360;
    const radians = angle * Math.PI / 180;
    const distance = 260 + (index % 6) * 42;
    spark.className = 'moon-particle moon-impact-spark';
    spark.style.setProperty('--dx', `${Math.cos(radians) * distance}px`);
    spark.style.setProperty('--dy', `${Math.sin(radians) * distance}px`);
    spark.style.setProperty('--spin', `${angle}deg`);
    spark.style.setProperty('--particle-colour', colours[index % colours.length]);
    spark.style.setProperty('--delay', `${(index % 4) * 0.015}s`);
    layer.appendChild(spark);
  }
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
      `<span>${row.place.toFixed(2)}</span>` +
      `<span>${row.ev.toFixed(1)}</span></div>`,
  ).join('');
  const panel = id('hint');
  panel.innerHTML =
    '<h2>Solver</h2>' +
    '<p class="hint-note">Place is your expected finish; 1 is a sole win, 4 is last. Round EV is expected penalty points; lower is better.</p>' +
    '<div class="hint-row hint-head"><span>Move</span><span>Place ↓</span><span>Round EV ↓</span></div>' +
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

#!/usr/bin/env ruby

require "csv"

SEATS = "NESW"
RANKS = "23456789TJQKA"

def mean(values)
  values.sum(0.0) / values.length
end

def paired_se(values)
  return 0.0 if values.length < 2

  average = mean(values)
  Math.sqrt(values.sum { |value| (value - average)**2 } / (values.length - 1) / values.length)
end

def card_rank(card)
  RANKS.index(card[0]) or abort "bad card #{card.inspect}"
end

def trick_winner(plays)
  led = plays.first.last[1]
  plays.select { |_, card| card[1] == led }.max_by { |_, card| card_rank(card) }.first
end

def trick_points(plays)
  plays.sum { |_, card| card[1] == "H" ? 1 : card == "QS" ? 13 : 0 }
end

def parse_tricks(trace)
  tricks = trace.split("/").map do |chunk|
    plays = chunk.scan(/([NESW])([2-9TJQKA][CDHS])/)
    abort "bad trick trace #{chunk.inspect}" unless plays.length == 4 && plays.flatten.join == chunk
    plays
  end
  abort "expected 13 tricks, got #{tricks.length}" unless tricks.length == 13
  tricks
end

def dangerous?(card)
  RANKS.index(card[0]) >= RANKS.index("Q") || card[1] == "H" && "TJ".include?(card[0])
end

path = ARGV.fetch(0, "run.csv")
lines = File.readlines(path)
provenance = lines.take_while { |line| line.start_with?("#") }.map(&:chomp)
rows = CSV.parse(lines.drop(provenance.length).join, headers: true)
abort "no completed rows in #{path}" if rows.empty?
abort "CSV is not pair-flushed" unless rows.length.even?

required = %w[pair seating direction cfr_seats north east south west shooter
              shooter_side mc_attempts mc_passes passes_nesw plays_by_trick cfr_payoff]
missing = required - rows.headers
abort "missing columns: #{missing.join(", ")}" unless missing.empty?

pair_payoffs = rows.group_by { |row| row["pair"] }.sort_by { |pair, _| Integer(pair) }.map do |pair, pair_rows|
  abort "pair #{pair} has #{pair_rows.length} rows" unless pair_rows.length == 2
  mean(pair_rows.map { |row| Float(row["cfr_payoff"]) })
end

points = {"cfr" => 0, "mc" => 0}
moons = {"cfr" => 0, "mc" => 0}
rows.each do |row|
  SEATS.chars.each_with_index do |seat, index|
    side = row["cfr_seats"].include?(seat) ? "cfr" : "mc"
    points[side] += Integer(row[%w[north east south west][index]])
  end
  side = row["shooter_side"]
  moons[side] += 1 if side
end

cfr_moons = rows.select { |row| row["shooter_side"] == "cfr" }
pass_fed = []
eligible = 0
dangerous_cards = 0
missed_pre8 = []
missed_sweep_window = []
missed_crossing = []
miss_points = Hash.new(0)

cfr_moons.each do |row|
  shooter = row["shooter"]
  shooter_index = SEATS.index(shooter) or abort "bad shooter #{shooter.inspect}"
  giver_offset = {"left" => -1, "right" => 1, "across" => 2}[row["direction"]]
  if giver_offset
    giver_index = (shooter_index + giver_offset) % 4
    giver = SEATS[giver_index]
    unless row["cfr_seats"].include?(giver)
      eligible += 1
      pass = row["passes_nesw"].split("/", -1).fetch(giver_index).scan(/../)
      fed = pass.select { |card| dangerous?(card) }
      unless fed.empty?
        pass_fed << row
        dangerous_cards += fed.length
      end
    end
  end

  tricks = parse_tricks(row["plays_by_trick"])
  hands = SEATS.chars.to_h { |seat| [seat, tricks.flatten(1).select { |play| play.first == seat }.map(&:last)] }
  shooter_points = 0
  row_missed = false
  row_sweep_window = false
  row_crossing = false

  tricks.each do |trick|
    crossing = trick_winner(trick) == shooter && shooter_points < 8 && shooter_points + trick_points(trick) >= 8
    current = []
    trick.each do |seat, card|
      if !current.empty? && !row["cfr_seats"].include?(seat) && shooter_points.positive? && shooter_points < 8 && trick_winner(current) == shooter
        led = current.first.last[1]
        winning_card = current.find { |play| play.first == shooter }.last
        can_beat = hands.fetch(seat).any? do |held|
          held[1] == led && held != "QS" && card_rank(held) > card_rank(winning_card)
        end
        actual_beats = card[1] == led && card_rank(card) > card_rank(winning_card)
        if can_beat && !actual_beats
          row_missed = true
          row_sweep_window ||= shooter_points >= 5
          row_crossing ||= crossing
          miss_points[shooter_points] += 1
        end
      end
      index = hands.fetch(seat).index(card) or abort "card #{card} missing from #{seat}'s hand"
      hands.fetch(seat).delete_at(index)
      current << [seat, card]
    end
    shooter_points += trick_points(trick) if trick_winner(trick) == shooter
  end
  abort "CFR shooter ended on #{shooter_points} points" unless shooter_points == 26
  missed_pre8 << row if row_missed
  missed_sweep_window << row if row_sweep_window
  missed_crossing << row if row_crossing
end

puts provenance
puts "#{rows.length} deals (#{pair_payoffs.length} pairs)"
puts format("deep-cfr payoff per seat-deal: %+.3f ± %.3f", mean(pair_payoffs), paired_se(pair_payoffs))
puts format("penalty points per deal: deep-cfr %.2f, mc %.2f", points["cfr"].fdiv(rows.length), points["mc"].fdiv(rows.length))
puts "moons: mc #{moons["mc"]} / cfr #{moons["cfr"]}"
puts "mc moon attempts: #{rows.sum { |row| Integer(row["mc_attempts"]) }} (shoot passes chosen: #{rows.sum { |row| Integer(row["mc_passes"]) }})"
puts "P4 dangerous = Q/K/A of any suit or T/J hearts"
puts format("P4 pass-fed CFR moons: %d/%d (%.1f%% of all; %d/%d eligible, %d dangerous cards)", pass_fed.length, cfr_moons.length, 100.0 * pass_fed.length / [cfr_moons.length, 1].max, pass_fed.length, eligible, dangerous_cards)
puts format("P5 missed pre-8 beat: %d/%d CFR moons (%.1f%%); on threshold-crossing trick: %d/%d (%.1f%%)", missed_pre8.length, cfr_moons.length, 100.0 * missed_pre8.length / [cfr_moons.length, 1].max, missed_crossing.length, cfr_moons.length, 100.0 * missed_crossing.length / [cfr_moons.length, 1].max)
puts format("P5 missed beat in swept 5–7 window: %d/%d CFR moons (%.1f%%)", missed_sweep_window.length, cfr_moons.length, 100.0 * missed_sweep_window.length / [cfr_moons.length, 1].max)
puts "P5 missed-beat decision points by shooter points: #{miss_points.sort.to_h}"

// Stateless bridge between hearts-engine and the brianberns/Hearts Deep CFR
// model. Reads one JSON request per line on stdin, rebuilds the equivalent
// Hearts.InformationSet by replaying the public history through Brian's own
// library, asks the remote model for an action, and answers on stdout:
//
//   in:  {"kind":"play","seat":"N","dir":"left","hand":["QS","TH"],
//         "outgoing":["2C","3C","4C"],"incoming":["AH","KH","QH"],
//         "plays":[["E","2C"],["S","5C"],...]}
//   out: {"card":"QS","legal":["2C","QS",...]}
//
// "legal" echoes Brian's legal-action set so the Rust side can detect any
// rules drift between the two engines. The endpoint (default Brian's live
// site) is argv[0]; point it at a local Hearts.Web.Harness for full runs.
module CfrShim.Program

open System
open System.Text.Json
open Fable.Remoting.DotnetClient
open PlayingCards
open Hearts

/// Mirror of Hearts.Web.IHeartsApi: the record's type/member names define
/// the remoting routes, so they must match the server exactly.
type IHeartsApi =
    {
        GetActionIndex : InformationSet -> Async<int>
        GetStrategy : InformationSet -> Async<float[]>
    }

/// Same route builder as Hearts.Web.Client: /Hearts/IHeartsApi/{method}.
let private routeBuilder typeName methodName =
    sprintf "/Hearts/%s/%s" typeName methodName

/// "QS"-style code. Card.ToString uses suit glyphs, so spell it out.
let private code (card : Card) =
    sprintf "%c%c" (Rank.toChar card.Rank) (Suit.toLetter card.Suit)

let private parseDir = function
    | "left" -> ExchangeDirection.Left
    | "right" -> ExchangeDirection.Right
    | "across" -> ExchangeDirection.Across
    | "hold" -> ExchangeDirection.Hold
    | s -> failwith $"Unexpected direction: {s}"

/// Rebuilds Brian's information set from the request. The dealer seat is
/// arbitrary (it is not part of the model's encoding), so West stands in.
let private toInfoSet (root : JsonElement) =
    let str (name : string) = root.GetProperty(name).GetString()
    let cards (name : string) =
        root.GetProperty(name).EnumerateArray()
            |> Seq.map (fun e -> Card.fromString (e.GetString()))
            |> set
    let seat = Seat.fromChar ((str "seat").[0])
    let dir = parseDir (str "dir")
    let hand : Hand = cards "hand"
    let outgoing : Pass = cards "outgoing"
    let plays =
        root.GetProperty("plays").EnumerateArray()
            |> Seq.map (fun e ->
                Seat.fromChar ((e[0].GetString()).[0]),
                Card.fromString (e[1].GetString()))
            |> Seq.toArray
    let deal = ClosedDeal.create Seat.West dir
    match str "kind" with
        | "pass" ->
            // his exchange removes each passed card from the hand at once
            InformationSet.create
                seat (hand - outgoing) (Some outgoing) None deal
        | _ ->
            let leader =
                plays
                    |> Array.tryHead
                    |> Option.map fst
                    |> Option.defaultValue seat
            let deal =
                (ClosedDeal.startPlay leader deal, plays)
                    ||> Array.fold (fun d (_, card) ->
                        ClosedDeal.addPlay card d)
            // Rules divergence: in Brian's engine the Q♠ also breaks
            // hearts; in hearts-engine only a heart does. The flag only
            // feeds his legality mask (not the model's input encoding),
            // so overriding it puts both engines under identical rules.
            let deal =
                { deal with
                    HeartsBroken =
                        plays
                            |> Array.exists (fun (_, card) ->
                                card.Suit = Suit.Hearts) }
            let passOpts =
                if dir = ExchangeDirection.Hold then None, None
                else Some outgoing, Some (cards "incoming" : Pass)
            InformationSet.create
                seat hand (fst passOpts) (snd passOpts) deal

[<EntryPoint>]
let main argv =
    let baseUrl =
        argv
            |> Array.tryHead
            |> Option.defaultValue "https://www.bernsrite.com"
    let proxy =
        Remoting.createApi baseUrl
            |> Remoting.withRouteBuilder routeBuilder
            |> Remoting.buildProxy<IHeartsApi>
    let mutable line = Console.ReadLine()
    while not (isNull line) do
        if line.Trim() <> "" then
            use doc = JsonDocument.Parse(line)
            let infoSet = toInfoSet doc.RootElement
            let card =
                match infoSet.LegalActions with
                    | [| only |] -> only   // spare the server a request
                    | actions ->
                        let index =
                            proxy.GetActionIndex infoSet
                                |> Async.RunSynchronously
                        actions[index]
            let legal =
                infoSet.LegalActions
                    |> Array.map (fun c -> sprintf "\"%s\"" (code c))
                    |> String.concat ","
            printfn """{"card":"%s","legal":[%s]}""" (code card) legal
            Console.Out.Flush()
        line <- Console.ReadLine()
    0

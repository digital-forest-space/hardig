# Hardig Demo Video Plan (2:30)

Target audience: Investors / judges. Focused on hardig.app with a brief agent mode mention.

## Scene 1 — The Problem (0:00–0:20)

**Screen:** hardig.app landing page (hero section)

**Voiceover:**
> "If you hold crypto in a DeFi position, you have two bad options: leave your wallet hot and exposed, or lock it away cold and lose daily access. Hardig eliminates that tradeoff."

**Action:** Slow scroll down the landing page, pausing on the three value-prop cards (Delegated Access, Portable Keys, Dead Man's Switch).

## Scene 2 — Create a Position (0:20–0:45)

**Screen:** Connect wallet -> Positions page -> Create Position form

**Voiceover:**
> "You start by creating a position on Solana. This locks your funds in a vault controlled by an NFT admin key that lands in your wallet. The vault holds navSOL through Nirvana Finance — an asset with a rising floor price and zero liquidation risk."

**Action:** Connect Phantom -> click Create Position -> name it "My Vault" -> confirm transaction -> show the new position card with balance.

## Scene 3 — Fund the Position (0:45–1:00)

**Screen:** Finance page — Buy operation

**Voiceover:**
> "Deposit SOL to buy navSOL. Your floor price only goes up, which means your borrowing power grows over time. Borrow against it at zero interest, repay whenever you want."

**Action:** Quick buy of SOL -> show updated deposited balance on the stat cards. Brief flash of borrow/repay panels to show they exist.

## Scene 4 — Delegate a Key (1:00–1:35)

**Screen:** Key Management page — Authorize New Key form

**Voiceover:**
> "Here's what makes Hardig different. You can mint a delegated key — a standard Solana NFT — and hand it to anyone. You choose exactly what it can do. This key can only buy. This one can only reinvest. You can even rate-limit how much it can spend per day. The admin key goes into cold storage. The delegated keys handle daily operations."

**Action:** Open Authorize Key -> select "Buy + Repay" permissions -> set a target wallet -> confirm -> show the new key appear in the keyring list with permission badges. Quick scroll showing multiple keys with different permission sets.

## Scene 5 — Recovery (1:35–1:55)

**Screen:** Recovery page

**Voiceover:**
> "And if the admin key is ever lost or compromised? Hardig has a built-in dead man's switch. Configure a recovery key with a lockout period. If the admin goes silent too long, the recovery key holder takes over automatically. No multisig coordination. No support tickets. Just math."

**Action:** Show recovery status card with lockout period configured. Don't fill the form — just show the configured state and the "Recovery available in 29d 22h" countdown.

## Scene 6 — Agent Mode (1:55–2:15)

**Screen:** Agents page on hardig.app -> quick cut to terminal

**Voiceover:**
> "This model is especially powerful for AI agents. Give an agent a scoped key with only the permissions it needs. It operates autonomously within those bounds. The CLI outputs structured JSON — status, balances, transaction results — purpose-built for programmatic access."

**Action:** Show the Agents page briefly (permission reference table). Cut to terminal showing:
```
hardig-tui ./agent-key.json status
hardig-tui ./agent-key.json buy --amount 0.5
```
with JSON output. Keep it fast — 3-4 seconds of terminal footage.

## Scene 7 — Close (2:15–2:30)

**Screen:** Back to landing page hero

**Voiceover:**
> "Hardig. Your keys. Your rules. Deployed on Solana mainnet today."

**Action:** Show hardig.app URL. Flash the position NFT in a Phantom wallet as the final frame.

## Production Notes

| Item | Detail |
|------|--------|
| Pre-record setup | Have a funded position with 2-3 delegated keys already created, plus one empty wallet for the live "create position" demo |
| Wallet | Phantom (most recognizable to judges) |
| Cluster | Mainnet (real deployed program) |
| Screen recording | 1920x1080, browser zoomed to ~125% for readability |
| Terminal | Dark theme, large font (18pt+), minimal prompt |
| Music | Subtle ambient/electronic underneath — no vocals |
| Cuts | Pre-record each scene separately, edit out transaction wait times (splice to confirmation) |
| Captions | Add lower-third labels for each concept ("Delegated Access", "Rate-Limited Key", "Dead Man's Switch") |

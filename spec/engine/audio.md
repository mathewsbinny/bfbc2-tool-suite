# Audio and voice-over: what the game plays, from where

How BFBC2 finds and plays a level's sound: where a wave's audio lives (in memory, streamed, on demand), what a level
bundle may and may not carry, the two crashes bad sound data gives, the announcer's banks and trigger sets per content
set, and how the executable raises voice-over events. The Wave resource and its audio codecs are layer 1
(`formats/audio.md`); this file is what the engine does with them. The ticket thresholds that raise ticket lines
belong to the game modes (the gameplay file); here is only the voice-over plumbing.

**Sources.** Project OPFOR BFBC2's Pacific War port (the crashes met while installing Wake Island, its audits of the
converted sounds against DICE's), its static reading of `BFBC2Game.exe`'s voice-over system (2026-11-14, nothing run
or patched), and the map editor's export rounds E3c / E3d, which read each content set's announcer from the game copy
(`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.12). **Nothing here was confirmed by listening in game** unless a rule
says so. "The game copy" is OPFOR's `game\` (README).

## 1. Where a wave's audio is

### SND-WAVE-1: IsStreaming says where the audio is

Status: **observed** · Variants: PC

A wave partition's `IsStreaming` is true exactly when its audio is streamed: the Wave resource holds a stub and the
audio is a `_waveDataSns_<n>` entry of a streaming bundle (WAVE-SEG-2). A converted wave whose audio was put in memory
must say `IsStreaming` false; one that keeps true may be silent (open).

[data: DICE's data: all 75 stock waves in Wake Island's bundle and all 793 waves BFBC2 and BF1943 both ship: IsStreaming true exactly when the audio is streamed; five OPFOR-converted ambiences broke it (in-memory audio, flag true)]
[doc: bfo:docs/research/opfor/WW2_SHADER_AUDIO_AUDIT.md:150-158]

### SND-WAVE-2: Streamed audio is the global streaming bundles' alone

Status: **observed** · Variants: PC

Streamed wave data lives in the global streaming bundles (`streaming_sounds-00`, `streaming_vo_<lang>-00`), which the
game streams from itself. Voice-over waves are 44-byte stubs in the commons' `vo/<lang>` bundles with their audio in
`streaming_sounds-00` (DICE streams every voice-over line, EALayer3).

[data: PC loop segments: 34,038 streamed stubs, all voice-over among them (WAVE-SEG-2); SP_01's Japanese chatter: stubs in mp_common's vo/<lang> bundles, data in streaming_sounds-00]
[doc: private:opfor-research/research/WW2_LOADING_SCREENS_VO_RESEARCH_2026-11-14.md:199-200] [doc: bfo:docs/research/opfor/WW2_SHADER_AUDIO_AUDIT.md:172]

### SND-STRM-1: Streaming wave data in a level bundle crashes the load (0x49EDF4)

Status: **observed** · Variants: PC

A `_waveDataSns_<n>` entry (typed `<non-resource>`) copied into a level bundle is read by the loader as a partition,
and the load crashes at 0x49EDF4 (Wake Island's first load). A level bundle must carry no streamed wave data; the
copy sources of an export or port leave the streaming bundles out. `mapexport check` refuses a staged bundle holding
such an entry (round QW2).

[exe: BFBC2Game.exe 0x49EDF4, the loader reading streamed wave data as a partition]
[doc: opfor:src/tools/ww2/wake_install.py:193-197] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1369]
[code: rust/bc2-levelwrite/src/export.rs:2682-2687] [code: rust/bc2-levelwrite/src/export.rs:2759-2764] [test: rust/bc2-levelwrite/src/export.rs::streamed_waves_named]

### SND-OD-1: On-demand waves play only from their own bundle's partitions

Status: **observed** · Variants: PC

A wave with `IsOnDemand` true has its audio in `async/ondemand_sounds-00` in the on-demand form, and the game loads it
itself when asked. DICE plays on-demand waves only from the partitions the on-demand bundle was built for: a copy of
such a wave in a level bundle resolves to nothing (SND-NULL-1). A level that needs one copies it as an ordinary wave,
`IsOnDemand` false, its audio unwrapped from the on-demand form into the level bundle.

[data: OPFOR's Wake Island install: copied on-demand waves of mp_common resolved to a null wave on the client; the unwrapped copies play]
[code: rust/bc2-install/src/levelbuild/levelfiles.rs:1146-1148] [doc: opfor:src/tools/ww2/wake_install.py:193-206]

## 2. Sound set-up at load

### SND-LOAD-1: The client sets up the sounds of every bundle a level loads

Status: **observed** · Variants: PC

At load the client sets up every sound partition of every bundle in the level's stack, used or not, so every wave a
loaded sound names must resolve under that stack (PREC-ORD-1): a common bundle registered only for a few of its
partitions brings all its sounds with it.

[data: OPFOR's Wake Island: with sp_common in its targets (as the Onslaught ports had) the client set up sp_common's AI XM8 burst, whose waves nam_common's deleted markers hide, and crashed (SND-NULL-1); without sp_common it loads]
[doc: opfor:src/tools/ww2/wake_install.py:98-101]

### SND-NULL-1: A null wave in a sampler crashes the client (0xFF2AC2)

Status: **observed** · Variants: PC

A sampler whose wave map entry resolves to no wave crashes the client at 0xFF2AC2. Recorded causes: waves hidden by a
deleted marker of a bundle before theirs (BNDL-DEL-2: `nam_common`'s markers hide `mp_common`'s and `sp_common`'s
weapon waves that BFBC2's sounds and MP_005's animation sound map name), and an on-demand wave copied out of its bundle
(SND-OD-1). Every wave reference of a loaded sound, and of the level's animation sound map (a root file found by the
level's name, PART-NAME-3), must resolve; OPFOR's WW2 levels take their animation sound map from a Vietnam level for
this reason.

[exe: BFBC2Game.exe 0xFF2AC2, client sound set-up]
[doc: opfor:src/tools/ww2/wake_install.py:98-101] [doc: opfor:src/tools/ww2/wake_install.py:147-149]
[code: rust/bc2-install/src/levelbuild/levelfiles.rs:1146-1148] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1369]

## 3. The announcer

### SND-VO-1: Lines, banks and trigger sets

Status: **observed** · Variants: PC

A voice-over line is a row of a trigger set (a `VOAsset` under `sound/vo_triggers/`) whose event and conditions
match; the line resolves through the line banks `sound/vo/vo_<lang>` (language-specific lines) or
`sound/vo/vo_common` (lines in a soldier's own foreign language). A level plays the trigger sets its
`LevelData.VoiceOvers` lists. Which soldier speaks is chosen by the kit blueprint's `VOTypes` (a list of voice-over
persons) matched against each trigger's person conditions (inflictor, victim, listener).

[data: BF1943's single trigger set ported to Wake Island: 74 events over 22 event types; Vietnam kits list a class person and a nation person; SP_01's Japanese chatter set has 15 event types]
[code: rust/bc2-levelwrite/src/profile.rs:57-67]
[doc: private:opfor-research/research/WW2_LOADING_SCREENS_VO_RESEARCH_2026-11-14.md:137-142] [doc: private:opfor-research/research/WW2_LOADING_SCREENS_VO_RESEARCH_2026-11-14.md:179-186]

### SND-VO-2: Each content set's announcer

Status: **verified** · Variants: PC

The stock levels of BFBC2, BFBC1 and Vietnam carry no line bank or trigger set of their own: BFBC2's come from
`mp_common`, BFBC1's from `mp2_common`'s own copies (loaded before `mp_common`'s), Vietnam's from `nam_common`'s own
copies and `nam/` trigger sets. OPFOR's WW2 levels carry their own banks (BF1943's lines merged into `vo_common` and
`vo_en`), trigger sets and an English voice-over bundle in their own folder.

[code: rust/bc2-levelwrite/src/profile.rs:68-81] [test: rust/bc2-levelwrite/tests/export_e3c.rs::each_sets_announcer]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1079-1087]

### SND-VO-3: Voice-over blocks: absent sources, present targets

Status: **verified** · Variants: PC

A stock level registers a `package.mft` block per language, `game:levels/<lv>/vo/<lang>-00.fbrb`, whose source is
absent from the game copy (BNDL-LOC-3) and whose targets are its set's common voice-over bundles, which are there
(BFBC2: `mp1_common`'s; BFBC1: `mp2_common`'s then `mp_common`'s; Vietnam: `nam_common`'s then `mp_common`'s). A
WW2 level has one block (`vo/en`) and its own `vo/en-00` bundle (BF1943's waves). A level made from a donor takes the
donor's blocks under its own name.

[code: rust/bc2-levelwrite/src/export.rs:245-257] [test: rust/bc2-levelwrite/tests/export_e3c.rs::each_sets_announcer]
[test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1096-1107]

### SND-VO-4: A level's own banks shadow the commons'

Status: **verified** · Variants: PC

The level block loads the level's own bundle before the commons (PREC-ORD-2), so banks and trigger sets in it win: a
WW2 level's own `vo_common` / `vo_en` make any level built on it speak BF1943's lines, whatever set it is declared.
The announcer partitions must be the target set's own load's: where the base's own bundle shadows them, the
target's versions go into the level's bundle first.

[code: rust/bc2-levelwrite/src/export.rs:1795-1806] [code: rust/bc2-levelwrite/src/export.rs:2836-2847]
[test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1231-1235]

### SND-VO-5: Level bundles hide other modes' trigger sets

Status: **observed** · Variants: PC

Every stock level's own bundle hides the trigger sets of the other modes with deleted markers (Squad Deathmatch's on
the Conquest levels); a mode level loads only its own mode's announcer.

[data: the stock multiplayer levels' own bundles: deleted markers over other modes' trigger sets among their markers; OPFOR's mp_001 holds two Squad Deathmatch trigger sets in its Package bundle]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1089-1094]

## 4. Voice-over events in the executable

### SND-EVT-1: The event enum

Status: **observed** · Variants: PC

`VOEventType` has 125 values, 0 (`ETLoiteringSquad`) to 124 (`ETNone`); its table (`.rdata` 0x01C33688, 24-byte
entries) holds each name at +0 and its value at +0x10, equal to the index. Only three data fields have the type:
`VOEvent.EventType` (+0x1C), `TicketCountPercentage.VOEvent` and `RankLevelData.VOEvent`. An event of another game
with no name in the enum (BF1943's `ETHeadshotKill`, `ETRoadkill`, `ETVehicleRepaired`, `ETSoldierSpawned` ...)
cannot be expressed in data.

[exe: BFBC2Game.exe enum table 0x01C33688 (125 entries, ETNone 124); VOEventType TypeInfo 0x01C047BC; VOEvent TypeInfo 0x01C04C54, EventType +0x1C, PauseTime +0x18, Rows +0x20]
[doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:9-28] [doc: private:opfor-research/research/WW2_LOADING_SCREENS_VO_RESEARCH_2026-11-14.md:150-155]

### SND-EVT-2: Two voice-over systems, one raise

Status: **observed** · Variants: PC

There is one raise function, `VoiceOverSystem::raise` at 0x637770 (thiscall: system, event type, inflictor player,
victim player, position, mode), called from 71 sites, and two systems of the same class: the server's (pointer at
[0x01566B6C]) and the client's ([0x01571408]). Either player may be null; the position may not. The game's AI raises
its events (contact, cover, readiness, orders ...) on the server system only.

[exe: BFBC2Game.exe raise 0x637770 (ret 0x14), constructor 0x633330 (0x60 bytes), server system [0x01566B6C], client system [0x01571408], AI forwarder 0x57D380]
[doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:30-66]

### SND-EVT-3: Matching by type, throttled by PauseTime

Status: **observed** · Variants: PC

The matcher (0x636010) walks every loaded `VOAsset`'s events and keeps those whose `EventType` equals the raised type
(no range check, no table by type), skipping one whose `PauseTime` has not passed since it last played; the matched
rows are queued on a timed list. A type no loaded asset has plays nothing and does no harm.

[exe: BFBC2Game.exe matcher 0x636010 (ret 0x10, returns the matched row count), play path 0x637380, sink 0x636C80]
[doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:43-48] [doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:126]

### SND-EVT-4: Which side raises which event

Status: **observed** · Variants: PC

The client system raises the capture-point lines (`ETWonCapturePoint` / `ETLostCapturePoint` on the replicated flag
state, the lost line for every enemy capture), the ticket lines (from the replicated ticket counter), soldier killed,
spotted, entering a vehicle and the attack / defend focus points. The server system raises round won / lost (when a
ticket counter reaches 0, with the first player found on each team), avenge and saviour kills, vehicle killed,
grenade thrown / incoming and soldier fire. In multiplayer the chatter players hear is the client set; a server
raise reaches the players through the speaking soldier (SND-EVT-5).

[exe: BFBC2Game.exe client: capture point 0x8C7190, ticket counter 0x8E9AF0, message handler 0x800CF0, focus points 0x76E430; server: round end 0x67E3F0, kill scoring 0x86C070, message handler 0x5756A0, 0x5ACA80]
[doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:61-89]

### SND-EVT-5: A server line is played through the speaking soldier's ghost

Status: **observed** · Variants: PC

The two systems are one base class (constructor 0x633330) with a server subclass (vtable 0x1416910, built by the
server's level load 0x567DF0 into the server context +0x54) and a client one (vtable 0x1431578, the client's level
load 0x829B00, client +0x30). A raise queues each matched line as a record of the players' ids (+0x154 of inflictor,
victim and listener) and a due time (sink 0x636C80); the system's vtable slot 1 plays a due line:

- **server** (0x66AD20): the speaker's player from the server's player table (server context +0x2C, +0x6C by id),
  its soldier (player vtable +0x30), then the server soldier's line set 0x6B93C0. The soldier holds one line at a
  time: while a line of higher priority is still playing (+0x33C, time left +0x338) the new one is refused; otherwise
  the line's five keys go to +0x324 - +0x334, its priority and length to +0x33C / +0x338, and the soldier's ghost is
  marked with the dirty bit 0x200 (0xC1D8D0 on +0x16C);
- **client** (0x8C4EF0): the same through the client's players (0x7EDA60, vtable +0x34) to the client soldier
  (0x8AE2F0), which plays it. The client soldier's ghost update (0x8BF720) calls 0x8AE2F0 with the five keys when the
  bit 0x200 is in the update.

So a server-raised line (round won / lost, avenge and saviour kills, vehicle killed, grenades, soldier fire, the AI's
chatter) is heard by every client that has the speaking soldier's ghost: on the in-process host its own client
included, with no separate voice-over message (SND-EVT-4's sites). A line whose speaker has no soldier (dead, not yet
spawned) or no player is dropped (0x66AD20 returns), and a soldier speaking a line blocks lower-priority ones.

[exe: BFBC2Game.exe base constructor 0x633330; server subclass vtable 0x1416910 (slot 1 0x66AD20), built in 0x567DF0 (a 0x60-byte object, stored into the server context +0x54); client vtable 0x1431578 (slot 1 0x8C4EF0), built in 0x829B00; sink 0x636C80 (player +0x154); server soldier line set 0x6B93C0 (`cmp ebp, [esi+0x33C]`, stores +0x320 - +0x334, `push 0x200; call 0xC1D8D0` at 0x6B9468); client soldier ghost 0x8BF720 (bit 0x200 -> 0x8AE2F0)]
[doc: private:pseudo/0063.c:5458-5549] [doc: private:pseudo/008B.c:11853-12145] [doc: private:pseudo/008A.c:10458-10562]

## 5. Vehicle sounds

### SND-VEH-1: Where a vehicle's sounds are named

Status: **observed** · Variants: PC, BF1943

A vehicle's engine is `EngineComponentData.SoundEffect` (the full graph: idle, revs, load, start / stop),
`SimpleEngineSoundEffect` (distant) and `SurfaceSoundEffect` (tracks, surfaces); its radio `RadioComponentData`;
its horn a `WeaponComponentData` named `Horn` whose firing's `FireSoundEffect` is the horn (no horn class); its warnings
`VehicleEntityData.*WarnSoundEffect`; turret traverse `ChildComponentData.SoundEffect`. Damage and death sounds live in
the effects, not on the vehicle. A null field is a silent vehicle (DICE's campaign Type 97 has all three engine
fields null).

[data: DICE's stock vehicles (the M151, the Mi-24) and BF1943's: the same fields in both games; DICE's sp_01 Type 97: three null engine fields]
[doc: bfo:docs/research/opfor/WW2_VEHICLE_SOUNDS_AUDIT_2026-11-15.md:151-157]

## Open questions

- **0xFF2AC2's second attribution.** One OPFOR note attributes 0xFF2AC2 also to the client's mesh set-up finding no
  program for a ported mesh's entry (`wake_install.py` 2351-2353); whether the address belongs to sound set-up,
  mesh set-up or a shared resource lookup is not isolated.
- **In-memory voice-over and streamed flags.** Whether a wave with `IsStreaming` true and in-memory audio plays, and
  whether in-memory voice-over (OPFOR's 476 BF1943 lines as EA-XAS) plays at all: no listening test yet.
- **Bank lookup order.** BF1943's Japanese side has the same 40 hashed line names in `vo_en` and `vo_common`; which
  bank the engine looks up first (English or Japanese speech) is unverified.
- **Server raises on the host.** How a server line reaches the player is settled statically (SND-EVT-5, round DQ7:
  through the speaking soldier's ghost). Open: whether the server system's database (its +4, which the matcher walks)
  holds the level's `VoiceOvers` (`LevelData.VoiceOvers` +0x32C; the filling is not traced; the `vo_probe` matcher count
  answers it) and a listening test on the host.
- **Null person conditions.** Whether a trigger whose person condition is a null reference matches every soldier or
  none.
- **Missing registered voice-over bundles.** Every stock level's `vo/<lang>` source is absent and the game runs; what
  the engine does with such a block (reads its targets only, or nothing).

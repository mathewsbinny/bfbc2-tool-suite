# The input system: actions, concepts, bindings, devices

How a key, mouse button or pad control becomes an action a soldier or vehicle performs on the PC client: the
enumerations, the three data layers (device -> concept -> entry input action), where the bindings are stored and
loaded, the controls screen, the PC's pad path, the tuning data, and where OPFOR's controller patch (MOD-IN-1) and a
bot module (BOT-CTL-2 - BOT-CTL-4) sit in the chain. The game settings the controls screen also writes are
[settings.md](settings.md).

**Sources.** Round SPG3 (2026-11-23): `BFBC2Game.exe` (PC build 795745) read statically on a scratch copy; DICE's
`Input/*` partitions of the startup bundle and of `mp_common`'s level bundle through `spec/probes/options.py`;
Project OPFOR BFBC2's controller research (`bfo:docs/plans/opfor/CONTROLLER_PLAN.md` section 2, `private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md`
section 10); the element and field **names** of one per-user input file (read only).

The chain, from device to server:

```
key / mouse / pad control --(InputActionsData of a concept, per user map: INP-DATA-1, INP-FILE-1)--> concept
concept --(InputActionMappingsData named by the player's entry: INP-DATA-2)--> EntryInputAction (Eia*)
EntryInputAction --> the entry's EntryInputState (49 actions) --> sent to the server like any remote player's
```

DICE's AI writes the last stage directly (BOT-CTL-2, BOT-CTL-3): bots have no concepts and no bindings.

## 1. The enumerations

### INP-ENUM-1: Fifty entry input actions

Status: **verified** · Variants: PC

`EntryInputActionEnum` (field table 0x01C27C58, records of 0x18 bytes, name at +0, value at +0x10) has 50 values:
0 `EiaThrottle`, 1 `EiaStrafe`, 2 `EiaBrake`, 3 `EiaHandBrake`, 4 `EiaYaw`, 5 `EiaPitch`, 6 `EiaRoll`, 7
`EiaSwitchPrimaryWeapon`, 8 `EiaFire`, 9 `EiaCameraPitch`, 10 `EiaCameraYaw`, 11 `EiaFireCountermeasure`, 12
`EiaAltFire`, 13 `EiaCycleRadioChannel`, 14 `EiaZoom`, 15 `EiaJump`, 16 `EiaChangeVehicle`, 17 `EiaChangeEntry`, 18 -
25 `EiaSelectEntry0` - `EiaSelectEntry7`, 26 `EiaChangePose`, 27 `EiaInteract`, 28 `EiaToggleParachute`, 29
`EiaReload`, 30 `EiaToggleCamera`, 31 `EiaSprint`, 32 `EiaIngameMenu`, 33 `EiaGrenadeLauncher`, 34
`EiaStaticGadget`, 35 `EiaDynamicGadget1`, 36 `EiaDynamicGadget2`, 37 `EiaMeleeAttack`, 38 `EiaThrowGrenade`, 39
`EiaOrder`, 40 `EiaScoreboardMenu`, 41 `EiaPushToTalk`, 42 `EiaSnapView`, 43 `EiaVoteYes`, 44 `EiaVoteNo`, 45 - 48
`EiaSayAllChat`, `EiaSayTeamChat`, `EiaSaySquadChat`, `EiaSayLeaderChat`, 49 `EiaUndefined`. An entry's input state
holds the 49 actions 0 - 48 (the AI clears exactly these, BOT-CTL-3).

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe EntryInputActionEnum field table 0x01C27C58 (50 records, EiaThrottle 0 ... EiaUndefined 49)]

### INP-ENUM-2: Seventy-seven input concepts

Status: **verified** · Variants: PC

A binding names a **concept**, not an action: the concept enumeration (field table from 0x01C2F140) has 77 values, 0
`ConceptMoveFB` ... 75 `ConceptPushToTalk`, 76 `ConceptUndefined`. Concepts are finer than actions (`ConceptMoveFB`
and the halves `ConceptMoveForward` / `ConceptMoveBackward`, `ConceptCrouch` / `ConceptChangePose` / `ConceptLie`,
nine `ConceptSelectInventoryItem*`, map and menu concepts the server never sees).

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe the concept enumeration's field table from 0x01C2F140 (77 records)]

### INP-ENUM-3: The device enumerations

Status: **observed** · Variants: PC, Xbox 360

The device side has its own enumerations, read from the same kind of field tables: keys `IDK_*` (values 0 `IDK_None`
- 255 `IDK_Undefined`, DirectInput scan codes), pad buttons `IDB_*` (18: `IDB_Lup` 0 ... `IDB_Rtrigger2` 17, the
Xbox 360 pad's controls in the console's names - `Rdown` A, `Rright` B, `Rleft` X, `Rup` Y, `Ltrigger2` / `Rtrigger2`
the shoulders, `alt` Back - and `IDB_Undefined` 18), mouse buttons `IDB_Button_0` - `_7`, axes `IDA_*` (24 and
`IDA_Undefined`: per stick X / Y and their positive and negative halves), hat switches `IDP_POV_0` - `_2`. The console
build's control names `IntStart` ... `IntRightThumbYDownNormalized` (46, `IntUndefined`, `IntSize`), the platforms
`IampXenon` / `IampPs3` / `IampAllPlatforms` and the slots `IAMESlot1` - `IAMESlot8` are in the PC exe too.

[exe: BFBC2Game.exe field tables: IDK_W 0x01C2FF68, IDB_Rdown 0x01C2FB48, IDA_Axis0Y 0x01C2F890, IampXenon 0x01C285B8, IAMESlot1 0x01C28600]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:79-97]

## 2. The data

### INP-DATA-1: Concept partitions: device bindings per concept

Status: **verified** · Variants: PC

A concept partition (`Input/<Name>InputConcepts`, in the startup bundle) is one `EntryInputActionMapsData`
(`ActionMapSettingsScheme`, `DefaultInputActionMap`, `InputActionMaps`) holding `EntryInputActionMapData` maps
(`Actions`, `PlatformSpecific` - `IampXenon` on PC too - and `Slot`); each action is an `InputActionsData`: a
`ConceptIdentifier` and its `InputActions`, any number of `KeyboardInputActionData` (`Key`), `MouseInputActionData`
(`Axis` or `Button`, `NormalizeInput`, `ScaleValue`, `SimulateJoystickAxis`, `RememberExcessInput`) and
`PadInputActionData` (`Axis`, `Button` or `Pov`, `NormalizeInput`, `ScaleValue`, `UseSquareInput`), each with
`IsAnalog` and `NegateValue` (a negated entry is an axis's minus half). DICE's eight:

| Partition | Concepts | Keyboard | Mouse | Pad |
|---|---|---|---|---|
| `BFBC2SoldierInputConcepts` | 43 | 58 | 45 | 53 |
| `BFBC2VehicleInputConcepts` | 41 | 53 | 29 | 39 |
| `BFBC2AirInputConcepts` | 42 | 54 | 27 | 29 |
| `BFBC2SharedInputConcepts` | 50 | 56 | 18 | 19 |
| `BFBC2DefaultInputConcepts` | 36 | 53 | 13 | 31 |
| `VeniceSoldierInputConcepts` | 38 | 42 | 12 | 23 |
| `VeniceTankInputConcepts` | 35 | 36 | 10 | 22 |
| `VeniceMissileInputConcepts` | 35 | 38 | 10 | 11 |

(Pad counts are instances, bound or not: the PC data binds a pad control for 14 of 43 soldier concepts, 12 of 41
land and 11 of 42 air, CONTROLLER_PLAN 2.1. The `Venice*` partitions are the campaign's WWII prologue's, `sp_01`: its
soldier, tank and missile.)

[test: core/tests/test_spec_probes.py::options]
[data: spec/probes/options.py, 2026-11-23: DICE's startup bundle, the eight concept partitions' instance counts above]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:51-78]

### INP-DATA-2: Mapping partitions: concept to entry input action, per player entry

Status: **verified** · Variants: PC

A mapping partition (`Input/InputMappings/<Name>InputMapping`, in the commons' level bundles) is one
`InputActionMappingsData` whose `Mappings` are `EntryInputActionMappingData` pairs (`ConceptIdentifier`,
`ActionIdentifier`). Each player entry (soldier, vehicle seat) names a concept partition and a mapping partition
(VEH-SEAT-2); the action the entry receives is the mapping's. DICE's: `BFBC2SoldierInputMapping` 48 pairs,
`BFBC2VehicleInputMapping` 42, `BFBC2AirInputMapping` 40, `BFBC2GunnerInputMapping` 38, `BFBC2UAVInputMapping` 19
(`mp_common`'s level bundle).

[test: core/tests/test_spec_probes.py::options]
[data: spec/probes/options.py, 2026-11-23: mp_common's level-00 bundle, the five mapping partitions' pairs]

### INP-CFG-1: The input configuration: four user maps and the controls screen's layout

Status: **verified** · Variants: PC

`Game.InputConfiguration` (SET-ENG-1) names the startup bundle's `Input/InputConfiguration`: one
`InputConfigurationAsset` with a `DefaultInputConceptDefinition` (`BFBC2DefaultInputConcepts`), stance options
(`TogglePoseAction`, `CrouchAction`, `CrouchAndHoldAction`, `ProneToStandOnJump`, `SprintReleaseTime` ...) and four
`UserConfigurableActionMaps`, each an `Id` (the user file's name, INP-FILE-1), a `NameId` (its title's
localisation key), its concept partition (`ActionMap`) and a `ConfigurationLayout` listing the controls screen's
editable rows:

| Id | Concept partition | Rows |
|---|---|---|
| `infantry` | `BFBC2SoldierInputConcepts` | 36 |
| `air` | `BFBC2AirInputConcepts` | 18 |
| `land` | `BFBC2VehicleInputConcepts` | 15 |
| `shared` | `BFBC2SharedInputConcepts` | 12 |

A row is a `OneOptionAxisEditableAction` (33 instances) or a `OneOptionButtonEditableAction` (49): a `Concept`, a
`Header` (its label's localisation key) and, for an axis, `Invert`. 81 of the 82 are listed; one is defined and in no
layout.

[test: core/tests/test_spec_probes.py::options]
[data: spec/probes/options.py, 2026-11-23: Input/InputConfiguration: 4 maps, 36 / 18 / 15 / 12 rows, 33 + 49 editable actions, 81 listed]

## 3. The user's bindings

### INP-FILE-1: One XML file per user map, used when its version is 11 or more

Status: **observed** · Variants: PC

At client input start-up (0x5B14C0, from 0x7B8C00) the game takes the input configuration (0x6E3E90 reads
`Game.InputConfiguration`) and, for each user map, loads `/data/local/input/<Id>.dbx` (0x59B340:
`<Documents>\BFBC2\input\infantry.dbx` ...). The file is DBX in its **XML instance form** (`<root><instances>`, read
by 0x466670; the XML instance rules XMLI-*): an `EntryInputActionMapDataVersion` (`MapDataVersion`,
`EntryInputActionMapData` - a reference written `GUID<guid>`) and the map's `EntryInputActionMapData`,
`InputActionsData` and device entries as in INP-DATA-1. The file's map is used when the version (+0x10 of the
loaded object) is at least 11 (0x5B1567: `cmp [eax+0x10], 0xB; jge`); a missing, unreadable or older file gives the
concept partition's `DefaultInputActionMap`. The maps are kept by `Id` (+0x38 the map in use, +0x54 the default).

All four files are written (0x5AE8F0 -> 0x5AE830 per map -> 0x59B420, the XML writer 0x47B1E0) when a key change is
confirmed or cancelled and on a reset (0x5B1810, 0x5B1920, 0x5B325E). The names are a static list at 0x1570EF0:
`shared`, `infantry`, `land`, `air`.

[exe: BFBC2Game.exe 0x5B14C0 (0x6E3E90; per map 0x59B340 at 0x5B155E; version test 0x5B1567; default 0x4FC6F0 / 0x13B3600; maps +0x38 / +0x54 via 0x5AD5C0); load 0x59B340 ("/data/local/input/" 0x1419E08 + name + ".dbx" 0x140BA60; 0x4DDC10 open; 0x466670 XML read); save 0x59B420 (0x4DDBF0, 0x47B1E0); 0x5AE8F0 (callers 0x5B1801, 0x5B1908, 0x5B1931, 0x5B325E); name list 0x1570EF0 ("shared" 0x141A160, "infantry" 0x141A154, "land" 0x141A14C, "air" 0x141A148)]
[data: the element and field names of one per-user input file (read only): EntryInputActionMapDataVersion with MapDataVersion and EntryInputActionMapData, one EntryInputActionMapData (PlatformSpecific, Slot), InputActionsData, Keyboard- / Mouse- / PadInputActionData]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:51-78]

### INP-UI-1: The controls screen's names

Status: **verified** · Variants: PC

The controls page uses the menu name table (APT-LOAD-3):

| Name | Id | Kind | Does |
|---|---|---|---|
| `editableInputList` | 0x13C | extern | the rows of a map's `ConfigurationLayout` (INP-CFG-1) |
| `currentInputMapping` | 0x10A | extern | the binding shown for a row |
| `controlsSliderValues`, `controlsToggleValues` | 0x13D, 0x13E | extern | the `GstInput` sliders and switches (INP-SET-1) |
| `controllerConnected` | 0xCF | extern | a pad is present (MENU-EVT-1) |
| `requestEditInput` | 0xC8 | command | `"<a>,<b>,<c>"`: start capturing a key for a row (0x963408) |
| `confirmKeySwitch`, `cancelKeySwitch` | 0xC6, 0xC7 | command | keep or drop the captured key (0x5B1810, 0x5B1920), then save the four files (INP-FILE-1) |
| `resetInputSettings` | 0xC9 | command | the defaults back |
| `controlsSliderMoved`, `controlInvertToggle` | 0xCC, 0xCD | command | a slider or an invert switch (SET-UI-2) |
| `setActiveInputScheme` | 0xAA | command | the control scheme (no DICE movie names it) |

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe name table 0x0143C898; dispatcher 0x95E680 cases 0xC6 -> 0x9633DA (0x5B1810), 0xC7 -> 0x9633F1 (0x5B1920), 0xC8 -> 0x963408, 0xC9 -> 0x9631BC, 0xCC -> 0x962F37, 0xCD -> 0x962D45, 0xAA -> 0x962B2C; extern switch 0x13C -> 0x95AF2E, 0x10A -> 0x95C93F, 0x13D -> 0x95A63B, 0x13E -> 0x95A558]

### INP-SET-1: The input game settings

Status: **observed** · Variants: PC

`GstInput` (SET-GS-1) holds `Sensitivity0` - `Sensitivity5` (0.5 by default), `Scheme0Sensitivity`, the control
schemes `Scheme0` - `Scheme4` (`InputType`, `FlipY`), `InputAcceleration`, `InvertScrollWheel`, `Vibration` and the
hidden `AlternativeHeliControls`. The function 0x7C3E70 (from the client's input update 0x596BA0) reads
`Sensitivity0`, `Sensitivity1`, `Sensitivity2`, `InvertScrollWheel` and `Sensitivity3` - `Sensitivity5` through the
float getter 0x80D0C0; `Sensitivity3` / `4` / `5` are the joystick sliders (`joy_sensitive`, `joy_yaw_factor`,
`joy_pitch_factor`), which reach the player input as a yaw and pitch scale (setters 0x7CC820 / 0x7CC840, 0.5 - 1.5)
applied to both pad sticks. `Vibration` is read by 0x8301D0, but no PC code drives a pad's motors (INP-PAD-1).
`ToggleCrouch` and `HoldToZoom` are both a `GstGameplay` setting (SET-GS-4) and a field of the engine's `Game`
object (SET-ENG-1).

[exe: BFBC2Game.exe 0x7C3E70 (caller 0x596CD2 in 0x596BA0; "Sensitivity0" - "5", "InvertScrollWheel" between 0x7C3F00 and 0x7C4A80; getter 0x80D0C0 at 0x7C401C ... 0x7C485A); setters 0x7CC820 / 0x7CC840; 0x8301D0 (caller 0x955C73) reads Vibration, Scheme*, AimAssist, Crouch]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:91-97]

### INP-SET-2: Which control writes which input setting

Status: **verified** · Variants: PC

The controls page's slider command `controlsSliderMoved` (0xCC) takes `"<value>,<slider>"`: the value through
`atof`, the slider by its name, stored as a float with no clamp (SET-PARSE-1) and saved at once (SET-UI-2):

| Slider | Setting |
|---|---|
| `mouse_sensitive` | `Sensitivity0` (the mouse's overall sensitivity) |
| `mouse_yaw_factor` | `Sensitivity1` (the mouse's yaw factor) |
| `mouse_pitch_factor` | `Sensitivity2` (the mouse's pitch factor) |
| `joy_sensitive`, `joy_yaw_factor`, `joy_pitch_factor` | `Sensitivity3`, `Sensitivity4`, `Sensitivity5` |
| any other name | `SensitivityUnknown` (a setting the code then creates and saves) |

So the three mouse sliders are one sensitivity and two axis factors, not one per vehicle class as round SPG3 inferred.
`controlsSliderValues` (0x13D) answers the six values in index order (`Sensitivity%d`, 0 - 5). The switch command
`controlInvertToggle` (0xCD) takes `"<switch>,<true|false>"`: `look` sets `Scheme0FlipY` and `Scheme1FlipY`, `fly`
`Scheme2FlipY`, `wheel` `InvertScrollWheel`, `accelerate` `InputAcceleration` (1 for `true`, else 0).

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe controlsSliderMoved 0x962F37 (split at "," 0x5175F0; `atof` IAT 0x14054EC at 0x962FDE; name compares 0x963002 - 0x9630C1; float setter 0x82F860 at 0x9630E4); controlsSliderValues 0x95A63B ("Sensitivity%d" 0x1440BB8, `sprintf`, getter 0x80D0C0, 6 values); controlInvertToggle 0x962D45 ("look" 0x1440E24, "fly" 0x1440E20, "wheel" 0x1440E18, "accelerate" 0x1440E0C; int setter 0x82FAB0 at 0x962E0B - 0x962EC9)]
[data: spec/probes/options.py, 2026-11-27: the pushed names of the two commands' compares, in code order]

## 4. Devices

### INP-PAD-1: The PC reads one pad through DirectInput; the console's XInput path is present but not connected

Status: **observed** · Variants: PC

The PC client reads pads through one global pad (0x0150A470): its initialiser (0x7E7D50) takes the first
DirectInput joystick and chooses a button profile by USB id (Xbox 360 `045E:028E`, four Logitech pads, or a generic
layout); its update 0x7CC4A0 (one caller, the client input 0x7D2480 at 0x7D2504) fills 18 button values in `IDB_`
order and four stick axes, which the client input merges with the keyboard and mouse. Through DirectInput an Xbox
pad's triggers share one axis. The exe also holds the console's XInput device (vtable 0x142F948): its constructor
0x7D6890 has no caller and no reference, and its motor setters 0x7CC010 / 0x7CC050 and its output switch 0x7BA010
have no caller either - the PC has no rumble. `Client.UseGlobalGamePadInput` (0 by default) is read in 0x7D2480. The
menus are told a pad is connected through the UI manager's flag +0xC3F29, which the PC never sets.

[exe: BFBC2Game.exe 0x7E7D50 (reached by the jump at 0x7E845C); 0x7CC4A0 (caller 0x7D2504); XInput device constructor 0x7D6890, motors 0x7CC010 / 0x7CC050, switch 0x7BA010: no call and no absolute reference in the round's call index and reference search; 0x7BA120 reached only by the jump at 0x7CBFFD; XINPUT1_3 imports IAT 0x1405A54 - 0x1405A5C, read only at 0xE42CE2 - 0xE42CEE]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:79-129] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:484-500]

### INP-TUNE-1: Dead zones, sensitivities and curves live in the data

Status: **observed** · Variants: PC, Xbox 360

There is no global dead zone setting. The tuning is data: a device entry's `ScaleValue`, `NormalizeInput` and
`UseSquareInput` (INP-DATA-1); a vehicle's per-axis `ThrottleDeadzone`, `BrakeDeadzone`, `YawDeadzone`,
`PitchDeadzone`, `RollDeadzone` and throttle / brake inertia (field table from 0x01C1A788); a camera's
`YawSensitivityNonZoomed` / `Zoomed`, `PitchSensitivityNonZoomed` / `Zoomed`, `AverageFilterFrames`,
`AccumulateInput` (from 0x01C2F020); the soldier's aimer (`DefaultHaloAimer`: an input polynomial, yaw / pitch speeds,
acceleration and aim assist's `Suck*` terms, zero on PC). On top of the data, the settings' sensitivity sliders and
the joystick scale (INP-SET-1). PC and Xbox 360 ship different aimer values for the same instances (the PC curve
turns a full stick about 30 times as fast as the console's).

[exe: BFBC2Game.exe field tables: the deadzone fields 0x01C1A788 - 0x01C1A7E8 (+0x28 - +0x38); the camera sensitivities 0x01C2F020 - 0x01C2F068 (+0xC0 - +0xD0); IsPadRumbleEnabled 0x01C142D8 (a Client field)]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:51-78] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:484-500]

## 5. OPFOR's controller patch in the chain

### INP-MOD-1: OPFOR replaces the pad read and writes the console layouts into the user files

Status: **implemented** · Variants: PC

OPFOR's controller support works at two points of the chain, leaving the concept and mapping layers as DICE built
them:

1. **The device read.** The DLL replaces the global pad's update (0x7CC4A0, MOD-IN-1) with an XInput read in the
   console's layout - each button separate, analog triggers, radial dead zones - and holds the joystick scale at 1
   while a pad is in use (it applies the console formula itself); it feeds two pad buttons the PC lacks
   (`IDB_Rtopleft`, `IDB_Rtopright`) for its LT tap / hold and Y toggle. The menus get pad events and a virtual
   mouse through the Apt manager (MENU-HUD-5).
2. **The bindings.** The import step `make_pad_bindings.py` writes the console's default pad layouts into the four
   per-user files (INP-FILE-1, keeping `MapDataVersion` 11 so the game uses them), resolving concept -> action
   through the PC mapping partitions (INP-DATA-2) and overriding PC concepts whose meaning differs (the inventory
   slots, `CyclePrimaryInventory`).

The button prompts (`extern.platform` and the HUD's key names) follow the device in use (MOD-IN-1). A bot module
skips the whole chain and writes the entry input state (BOT-CTL-4).

[code: opfor:src/tools/controller/make_pad_bindings.py:1-16]
[code: opfor:src/dll/input.cpp:211-252]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:192-209]

## 6. Open questions

- Answered by round SPG3-rest: `Sensitivity0` - `Sensitivity2` are the mouse's sensitivity and its yaw and pitch
  factors (INP-SET-2). How 0x7C3E70 combines them into the turn rate was not followed.
- `Scheme0InputType` ... `Scheme4InputType`: the values a scheme takes and what `setActiveInputScheme` (which no DICE
  movie calls) switches.
- The one editable action in no layout (INP-CFG-1), and which concepts have no PC binding at all.
- How the game merges a user file that lacks a concept its default has (OPFOR's import adds missing concepts rather
  than rely on a merge); needs a test start.
- The stance options of `InputConfigurationAsset` against `ToggleCrouch`: which wins when both are set.

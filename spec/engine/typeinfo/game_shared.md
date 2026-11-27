# `GameSharedResources`: the game's data types, record by record

**Sources.** Round SPG4-rest (2026-11-27): `spec/probes/typeinfo.py` over `BFBC2Game.exe` (RTTI-REG-1, RTTI-MEM-1).
`GameSharedResources` is the game's own data module: 777 records, 350 of them DBX classes of DICE's data. The tables
are generated (do not edit them by hand); each lists the record, its parent, its size in bytes and its own members
with their run-time offsets (a member of a class type is a 4-byte reference; `T[]` an array), or an enumeration's
values. Inherited members are the parent's row. What the game does with a type is in the subsystem's file
(`engine/weapons.md`, `soldier.md`, `bot_ai.md` ...); these rules fix the layout the loader fills and the offsets
the code reads (the offsets `engine/*.md` cite are these).

## 1. Enumerations and value types

### RTTI-GSR-1: The 131 enumerations and their values

Status: **verified** · Variants: PC

DICE's DBX stores an enumeration field by its value's name (TYPE-VAL-4); the number is what the run-time object holds
and what the code compares. The values are not always 0..n-1 in declaration order: in 20 enumerations of the executable they are not
(here `InputDeviceKeys`, `MenuIdEnum`, `FindNearestMethod`, `PoseRestriction`, `ReadinessRestriction`), so an editor or
converter writing a value must take its number from this table, not from the name's position. The bot- and spawn-relevant ones: `SquadClass`, `SoldierRole`, `ReadinessLevel`,
`ReadinessState`, `CoverType`, `CoverState`, `MovementState`, `NavigationType`, `RouteType`, `AreaOrderType`,
`TeamAssignMode`, `SoldierSpawnInclude`, `AreaTriggerInclude`, `FactionId`, `SpecateMode` / `SpecateTarget` (the kill
cam, HUD-KILL-2).

<!-- typeinfo:gsr-enums -->
| Enumeration | Values |
|---|---|
| AchievementGroup | AchievementGroup_singlePlayer 0, AchievementGroup_multiPlayer 1 |
| AimControlMode | AcmFollowsMovement 0, AcmFollowsHead 1, AcmDependsOnReadinessAndAimState 2 |
| AimOption | AimOption_ApplyAlways 0, AimOption_ApplyOnlyWhenAiming 1, AimOption_ApplyOnlyWhenNotAiming 2 |
| AimOrigin | AoDefault 0, AoCamera 1, AoWeapon 2 |
| AimState | ASAiming 0, ASNotAiming 1, ASAny 2 |
| AmbientModelInterpolationType | AmitLinear 0, AmitSpline 1 |
| AmbientModelRotationType | AmrtStatic 0, AmrtDirectionOfFlight 1, AmrtWaypointTransform 2 |
| AnimatedAimingEnum | AnimatedAimingTwoHanded 0, AnimatedAimingRightHanded 1, AnimatedAimingDisabled 2 |
| AnimatedFireEnum | AnimatedFireAutomatic 0, AnimatedFireSingle 1, AnimatedFireHoldAndRelease 2, AnimatedFireSimple 3, AnimatedFireMelee 4 |
| AreaOrderType | AreaMoveTo 0, AreaHoldGround 1 |
| AreaTriggerInclude | ATNone 0, ATAll 1, ATPlayers 2, ATAI 3, ATAI_Allies 4, ATAI_Enemies 5 |
| ArtilleryStrikeType | AstRain 0, AstGuided 1, AstUnGuided 2, AstCount 3 |
| AwardGroup | AwardGroup_Achievement 0, AwardGroup_Pin 1, AwardGroup_Star 2, AwardGroup_Insignia 3 |
| AwardType | AwardType_OnceGlobally 0, AwardType_OncePerRound 1, AwardType_Unlimited 2, AwardType_Limited 3 |
| ChangeGameSettingType | CGSNone 0, CGSInvertVerticalLookAxis 1, CGSCount 2 |
| CharacterPoseType | CPTNoAction 0, CPTForceToStand 1, CPTForceToCroch 2 |
| ClearType | CTDestroy 0, CTUnspawn 1 |
| CoverEdge | CENeighbor 0, CESpace 1, CEBlocked 2 |
| CoverManipulationType | CMTRemoveCovers 0, CMTBlockEdges 1 |
| CoverSelectionMethod | CSMAgainstThreatPosition 0, CSMAgainstAllEnemies 1, CSMAgainstAllEnemiesWithFallback 2 |
| CoverState | CSInCover 0, CSAtLeftCorner 1, CSAtRightCorner 2, CSOutOfCover 3, CSAny 4 |
| CoverType | CTSoft 0, CTMedium 1, CTHard 2 |
| CriteriaType | CriteriaType_IAR_InARound 0, CriteriaType_IAS_InASpawn 1, CriteriaType_GLOBAL_AllTimeTotal 2, CriteriaType_IfNotTrue 3, CriteriaType_IAD_InADeathStreak 4 |
| DebugMenuItemType | DmitItem 0, DmitToggle 1 |
| DisableTeamType | DttDisable 0, DttUnspawn 1 |
| DogTagType | DogTagType_gold 0, DogTagType_silver 1, DogTagType_bronze 2 |
| EdgeModelType | EmtGeneral 0, EmtCorner 1 |
| EffectComponentTrigger | ectInit 0, ectSpawn 1, ectUnSpawn 2, ectPlayerEnter 3, ectPlayerExit 4, ectCameraChange 5, ectCameraChange1P 6, ectCameraChange3P 7, ectCriticallyDamaged 8 |
| EffectPriority | EpLow 0, EpHigh 1 |
| EnterVehicleRestriction | EVRNoRestriction 0, EVRAiOnly 1, EVRSeatSpecific 2, EVRSeatToBan 3, EVRHumanOnly 4, EVRRestrictAll 5, EVTBanAiFromSeat 6 |
| EntryClass | ecPrimary 0, ecSecondary 1 |
| EntryEnterRestriction | eerNoRestriction 0, eerOnlyPrimaryEntry 1, eerNotWhenReloadingWeapon 2 |
| EntryInputActionEnum | EiaThrottle 0, EiaStrafe 1, EiaBrake 2, EiaHandBrake 3, EiaYaw 4, EiaPitch 5, EiaRoll 6, EiaSwitchPrimaryWeapon 7, EiaFire 8, EiaCameraPitch 9, EiaCameraYaw 10, EiaFireCountermeasure 11, EiaAltFire 12, EiaCycleRadioChannel 13, EiaZoom 14, EiaJump 15, EiaChangeVehicle 16, EiaChangeEntry 17, EiaSelectEntry0 18, EiaSelectEntry1 19, EiaSelectEntry2 20, EiaSelectEntry3 21, EiaSelectEntry4 22, EiaSelectEntry5 23, EiaSelectEntry6 24, EiaSelectEntry7 25, EiaChangePose 26, EiaInteract 27, EiaToggleParachute 28, EiaReload 29, EiaToggleCamera 30, EiaSprint 31, EiaIngameMenu 32, EiaGrenadeLauncher 33, EiaStaticGadget 34, EiaDynamicGadget1 35, EiaDynamicGadget2 36, EiaMeleeAttack 37, EiaThrowGrenade 38, EiaOrder 39, EiaScoreboardMenu 40, EiaPushToTalk 41, EiaSnapView 42, EiaVoteYes 43, EiaVoteNo 44, EiaSayAllChat 45, EiaSayTeamChat 46, EiaSaySquadChat 47, EiaSayLeaderChat 48, EiaUndefined 49 |
| EventSendOption | ESONoChange 0, ESOEnable 1, ESODisable 2 |
| FactionId | FactionNeutral 0, FactionUS 1, FactionRUS 2, FactionMEC 3, FactionIdCount 4 |
| FakePoseLimbs | FakePoseLimbs_Head 0, FakePoseLimbs_LeftUpArm 1, FakePoseLimbs_LeftDownArm 2, FakePoseLimbs_RightUpArm 3, FakePoseLimbs_RightDownArm 4, FakePoseLimbs_UpSpine 5, FakePoseLimbs_DownSpine 6, FakePoseLimbs_LeftUpLeg 7, FakePoseLimbs_LeftDownLeg 8, FakePoseLimbs_RightUpLeg 9, FakePoseLimbs_RightDownLeg 10, FakePoseLimbs_Count 11 |
| FindNearestMethod | FnmKynapse 1, FnmFrostbite 2 |
| FireLogicType | fltSingleFire 0, fltSingleFireWithBoltAction 1, fltAutomaticFire 2, fltHoldAndRelease 3, fltDetonatedFiring 4 |
| FireState | FSFireNearby 0, FSNoFire 1, FSAny 2 |
| ForceType | FtSphere2D 0, FtSphere3D 1, FtDirection2D 2, FtDirection3D 3 |
| FrameEffectBlendMode | FebmAdd 0, FebmMultiply 1 |
| GameSettingsType | GstAudio 0, GstGraphics 1, GstRender 2, GstInput 3, GstAI 4, GstGameplay 5, GstPlayerProfile 6, GstPersistence 7, GstBinary 8, GstMenuTracker 9, GstStatsCache 10, GstCount 11 |
| HudCounterType | HctBar 0, HctNumber 1 |
| ImmortalType | ItNone 0, ItGod 1, ItJesus 2 |
| InputActionMapPlatform | IampXenon 0, IampPs3 1, IampAllPlatforms 2 |
| InputActionMapSlot | IAMESlot1 0, IAMESlot2 1, IAMESlot3 2, IAMESlot4 3, IAMESlot5 4, IAMESlot6 5, IAMESlot7 6, IAMESlot8 7, IAMESlotCount 8 |
| InputConceptIdentifiers | ConceptMoveFB 0, ConceptMoveLR 1, ConceptMoveForward 2, ConceptMoveBackward 3, ConceptMoveLeft 4, ConceptMoveRight 5, ConceptYaw 6, ConceptPitch 7, ConceptRoll 8, ConceptFire 9, ConceptAltFire 10, ConceptFireCountermeasure 11, ConceptReload 12, ConceptZoom 13, ConceptToggleCamera 14, ConceptSprint 15, ConceptJump 16, ConceptCrouch 17, ConceptChangePose 18, ConceptLie 19, ConceptInteract 20, ConceptPickUp 21, ConceptDrop 22, ConceptNextInventoryItem 23, ConceptPrevInventoryItem 24, ConceptCyclePrimaryInventory 25, ConceptCycleSecondaryInventory 26, ConceptSelectInventoryItem1 27, ConceptSelectInventoryItem2 28, ConceptSelectInventoryItem3 29, ConceptSelectInventoryItem4 30, ConceptSelectInventoryItem5 31, ConceptSelectInventoryItem6 32, ConceptSelectInventoryItem7 33, ConceptSelectInventoryItem8 34, ConceptSelectInventoryItem9 35, ConceptChangeVehicle 36, ConceptBrake 37, ConceptHandBrake 38, ConceptClutch 39, ConceptGearUp 40, ConceptGearDown 41, ConceptNextPosition 42, ConceptSelectEntry0 43, ConceptSelectEntry1 44, ConceptSelectEntry2 45, ConceptSelectEntry3 46, ConceptSelectEntry4 47, ConceptSelectEntry5 48, ConceptSelectEntry6 49, ConceptSelectEntry7 50, ConceptCameraPitch 51, ConceptCameraYaw 52, ConceptMapZoom 53, ConceptMapSize 54, ConceptMapThreeDimensional 55, ConceptScoreboard 56, ConceptMenu 57, ConceptCancel 58, ConceptCommMenu1 59, ConceptCommMenu2 60, ConceptAccept 61, ConceptDecline 62, ConceptSelect 63, ConceptSpawnMenu 64, ConceptLargeMap 65, ConceptChatMenu 66, ConceptVoiceFunction1 67, ConceptSayAllChat 68, ConceptTeamChat 69, ConceptSquadChat 70, ConceptSquadLeaderChat 71, ConceptSelectMeleeWeapon 72, ConceptToggleParachute 73, ConceptSnapViewCamera 74, ConceptPushToTalk 75, ConceptUndefined 76 |
| InputDeviceAxes | IDA_Axis0X 0, IDA_Axis0Y 1, IDA_Axis0XPos 2, IDA_Axis0YPos 3, IDA_Axis0XNeg 4, IDA_Axis0YNeg 5, IDA_Axis1X 6, IDA_Axis1Y 7, IDA_Axis1XPos 8, IDA_Axis1YPos 9, IDA_Axis1XNeg 10, IDA_Axis1YNeg 11, IDA_Axis2X 12, IDA_Axis2Y 13, IDA_Axis3X 14, IDA_Axis3Y 15, IDA_Axis4X 16, IDA_Axis4Y 17, IDA_Axis5X 18, IDA_Axis5Y 19, IDA_Axis6X 20, IDA_Axis6Y 21, IDA_Axis7X 22, IDA_Axis7Y 23, IDA_Undefined 24 |
| InputDeviceKeys | IDK_None 0, IDK_Escape 1, IDK_1 2, IDK_2 3, IDK_3 4, IDK_4 5, IDK_5 6, IDK_6 7, IDK_7 8, IDK_8 9, IDK_9 10, IDK_0 11, IDK_Minus 12, IDK_Equals 13, IDK_Backspace 14, IDK_Tab 15, IDK_Q 16, IDK_W 17, IDK_E 18, IDK_R 19, IDK_T 20, IDK_Y 21, IDK_U 22, IDK_I 23, IDK_O 24, IDK_P 25, IDK_LeftBracket 26, IDK_RightBracket 27, IDK_Enter 28, IDK_LeftCtrl 29, IDK_A 30, IDK_S 31, IDK_D 32, IDK_F 33, IDK_G 34, IDK_H 35, IDK_J 36, IDK_K 37, IDK_L 38, IDK_Semicolon 39, IDK_Apostrophe 40, IDK_Grave 41, IDK_LeftShift 42, IDK_Backslash 43, IDK_Z 44, IDK_X 45, IDK_C 46, IDK_V 47, IDK_B 48, IDK_N 49, IDK_M 50, IDK_Comma 51, IDK_Period 52, IDK_Slash 53, IDK_RightShift 54, IDK_Multiply 55, IDK_LeftAlt 56, IDK_Space 57, IDK_Capital 58, IDK_F1 59, IDK_F2 60, IDK_F3 61, IDK_F4 62, IDK_F5 63, IDK_F6 64, IDK_F7 65, IDK_F8 66, IDK_F9 67, IDK_F10 68, IDK_Numlock 69, IDK_ScrollLock 70, IDK_Numpad7 71, IDK_Numpad8 72, IDK_Numpad9 73, IDK_Subtract 74, IDK_Numpad4 75, IDK_Numpad5 76, IDK_Numpad6 77, IDK_Add 78, IDK_Numpad1 79, IDK_Numpad2 80, IDK_Numpad3 81, IDK_Numpad0 82, IDK_Decimal 83, IDK_OEM_102 86, IDK_F11 87, IDK_F12 88, IDK_F13 100, IDK_F14 101, IDK_F15 102, IDK_Kana 112, IDK_Convert 121, IDK_NoConvert 123, IDK_Yen 125, IDK_NumpadEquals 141, IDK_PrevTrack 144, IDK_At 145, IDK_Colon 146, IDK_Underline 147, IDK_Kanji 148, IDK_Stop 149, IDK_Ax 150, IDK_Unlabeled 151, IDK_NextTrack 153, IDK_NumpadEnter 156, IDK_RightCtrl 157, IDK_Mute 160, IDK_Calculator 161, IDK_PlayPause 162, IDK_MediaStop 164, IDK_VolumeDown 174, IDK_VolumeUp 176, IDK_WebHome 178, IDK_NumpadComma 179, IDK_Divide 181, IDK_PrintScreen 183, IDK_RightAlt 184, IDK_Pause 197, IDK_Home 199, IDK_ArrowUp 200, IDK_PageUp 201, IDK_ArrowLeft 203, IDK_ArrowRight 205, IDK_End 207, IDK_ArrowDown 208, IDK_PageDown 209, IDK_Insert 210, IDK_Delete 211, IDK_LeftWin 219, IDK_RightWin 220, IDK_AppMenu 221, IDK_Power 222, IDK_Sleep 223, IDK_Wake 227, IDK_WebSearch 229, IDK_WebFavorites 230, IDK_WebRefresh 231, IDK_WebStop 232, IDK_WebForward 233, IDK_WebBack 234, IDK_MyComputer 235, IDK_Mail 236, IDK_MediaSelect 237, IDK_Undefined 255 |
| InputDeviceMouseButtons | IDB_Button_0 0, IDB_Button_1 1, IDB_Button_2 2, IDB_Button_3 3, IDB_Button_4 4, IDB_Button_5 5, IDB_Button_6 6, IDB_Button_7 7, IDB_Button_Undefined 8 |
| InputDevicePOVs | IDP_POV_0 0, IDP_POV_1 1, IDP_POV_2 2, IDP_POV_Undefined 3 |
| InputDevicePadButtons | IDB_Lup 0, IDB_Ldown 1, IDB_Lleft 2, IDB_Lright 3, IDB_Rup 4, IDB_Rdown 5, IDB_Rleft 6, IDB_Rright 7, IDB_Rtopleft 8, IDB_Rtopright 9, IDB_Lthumb 10, IDB_Rthumb 11, IDB_start 12, IDB_alt 13, IDB_Ltrigger 14, IDB_Rtrigger 15, IDB_Ltrigger2 16, IDB_Rtrigger2 17, IDB_Undefined 18 |
| InputNodeType | IntStart 0, IntAlt 1, IntButtonUp 2, IntButtonDown 3, IntButtonLeft 4, IntButtonRight 5, IntDpadUp 6, IntDpadDown 7, IntDpadLeft 8, IntDpadRight 9, IntLeftTrigger1 10, IntLeftTrigger2 11, IntRightTrigger1 12, IntRightTrigger2 13, IntLeftThumbButton 14, IntRightThumbButton 15, IntLeftThumbX 16, IntLeftThumbY 17, IntLeftThumbXSquared 18, IntLeftThumbYSquared 19, IntRightThumbX 20, IntRightThumbY 21, IntRightThumbXSquared 22, IntRightThumbYSquared 23, IntRightThumbXNormalized 24, IntRightThumbYNormalized 25, IntLeftThumbXLeft 26, IntLeftThumbXRight 27, IntLeftThumbYUp 28, IntLeftThumbYDown 29, IntLeftThumbXLeftSquared 30, IntLeftThumbXRightSquared 31, IntLeftThumbYUpSquared 32, IntLeftThumbYDownSquared 33, IntRightThumbXLeft 34, IntRightThumbXRight 35, IntRightThumbYUp 36, IntRightThumbYDown 37, IntRightThumbXLeftSquared 38, IntRightThumbXRightSquared 39, IntRightThumbYUpSquared 40, IntRightThumbYDownSquared 41, IntRightThumbXLeftNormalized 42, IntRightThumbXRightNormalized 43, IntRightThumbYUpNormalized 44, IntRightThumbYDownNormalized 45, IntUndefined 46, IntSize 47 |
| InteractionEntityType | IEDefault 0, IEGold 1, IEArmSDK 2, IEDefuseSDK 3, IEDisarmTrap 4, IERelease 5, IETriangulate 6, IESyncData 7 |
| KitCategory | KCAssault 0, KCDemolition 1, KCRecon 2, KCSpecialist 3, KCSupport 4, KCRecruit 5, KCRifleman 6, KCAntitank 7, KCMachinegunner 8, KCSpecialforces 9, KCSniper 10, KCFieldworks 11, KCCombatmedic 12, KCUnknown 13, KCUCount 14 |
| LockType | LockAlways 0, LockOnTagged 1, LockNever 2 |
| LpfStreamingMethod | LsmDirect 0, LsmGrid 1 |
| MapMarkerBlinkType | MMBTHold 0, MMBTBlink 1, MMBTCount 2 |
| MapMarkerColorType | MMCTRed 0, MMCTBlue 1, MMCTGreen 2, MMCDefault 3 |
| MapMarkerType | MMTMissionObjective 0, MMTSecondaryMissionObjective 1, MMTSoldier 2, MMTVehicle 3, MMTAmmoCrate 4, MMTGeneric 5, MMTMultiplayerObjective1 6, MMTMultiplayerObjective2 7 |
| MatchMakingMatchingOption | MatchMakingMatchingOption_Normal 0, MatchMakingMatchingOption_MatchOnly 1 |
| MatchMakingPlaylistRule | MatchMakingPlaylistRule_Random 0, MatchMakingPlaylistRule_Linear 1 |
| MatchMakingPreferenceVariant | MatchMakingPreferenceVariant_Normal 0, MatchMakingPreferenceVariant_PingSite 1, MatchMakingPreferenceVariant_Dynamic 2 |
| MatchMakingSessionType | MatchMakingSessionType_FindServer 0, MatchMakingSessionType_AssignHost 1, MatchMakingSessionType_ResetServer 2, MatchMakingSessionType_ListServers 3 |
| MatchMakingVotingMethod | MatchMakingVotingMethod_Plurality 0, MatchMakingVotingMethod_Lottery 1 |
| MenuIdEnum | MieSpawnMenu 2, MiePlaying 4, MieSpectate 5, MieKilledMenu 6, MieZoomMinimap 8, MiePreSpawn 9, MiePostSpawn 10, MieDeath 11, MieVictoryScreen 12, MieLetterBox 13, MieMovie 14, MieMissionFailure 15, MieVoid 16, MieDefault 17, MieCreateSoldier 18, MiePlayingNoMinimap 19, MieLastMenuId 20 |
| MessageReciever | MrAll 0, MrTeam 1, MrTeamAndSquad 2, MrPlayer 3 |
| MovementState | MsMoving 0, MsStopped 1, MsAny 2 |
| NavigationType | NTGround 0, NTHelicopter 1, NTNone 2 |
| ObjectiveType | OTPrimary 0, OTTactical 1, OTChildMisson 2 |
| OnlineProviderPlatform | OnlineProviderPlatform_Win32 0, OnlineProviderPlatform_Xenon 1, OnlineProviderPlatform_PS3 2 |
| OrderType | otNone 0, otRequestHeal 1, otLetMeHeal 2, otRequestAmmo 3, otLetMeGiveAmmo 4, otLetMeRepair 5, otRequestRide 6, otSocialise 7, otOrderCount 8 |
| PathdataCollisionModel | PCMSphere 0, PCMSweptSphere 1, PCMCylinder 2 |
| PathdataGenerator | PgFrostbiteMapBuilder 0, PgVehicleMapBuilder 1 |
| PathfindingAccidentDetection | PadPredictMove 0, PadVehicle 1 |
| PathfindingDisableType | PdtNever 0, PdtWhenAlive 1, PdtWhenDestroyed 2, PdtWhenPartDestroyed 3 |
| PersistentSpecializationType | PersistentSpecializationType_None 0, PersistentSpecializationType_AssaultRifleSpecialist 1, PersistentSpecializationType_LightMachineGunSpecialist 2, PersistentSpecializationType_SniperRifleSpecialist 3, PersistentSpecializationType_VehicleAltFire 4, PersistentSpecializationType_VehicleCountermeasure 5, PersistentSpecializationType_VehicleZoom 6, PersistentSpecializationType_MarksmanSpecialist 7, PersistentSpecializationType_DemolitionsSpecialist 8, PersistentSpecializationType_SoldierWeaponZoom 9, PersistentSpecializationType_ShotgunSpecialist 10, PersistentSpecializationType_ExplosiveSupplySpecialist 11, PersistentSpecializationType_GrenadeSupplySpecialist 12, PersistentSpecializationType_AmmunitionSupplySpecialist 13, PersistentSpecializationType_HealthSpecialist 14, PersistentSpecializationType_SprintSpecialist 15, PersistentSpecializationType_AimSpecialist 16, PersistentSpecializationType_HardenedVehicleArmor 17, PersistentSpecializationType_VehicleMotionSensor 18, PersistentSpecializationType_WeaponScope 19, PersistentSpecializationType_WeaponScopeCobra 20, PersistentSpecializationType_ZoomSpotSpecialist 21, PersistentSpecializationType_MedicHealRadius 22, PersistentSpecializationType_MedicHealPower 23, PersistentSpecializationType_VehicleDamageSpecialist 24, PersistentSpecializationType_VehicleReloadSpecialist 25, PersistentSpecializationType_DO_NOT_USE_THIS_TYPE_LAST_COUNT 26 |
| PersistentValueProtection | PersistentValueProtection_ReadOnly 0, PersistentValueProtection_WriteOnly 1, PersistentValueProtection_ReadAndWrite 2 |
| PersistentValueType | PersistentValueType_Set 0, PersistentValueType_High 1, PersistentValueType_Low 2, PersistentValueType_Increment 3, PersistentValueType_Decrement 4 |
| PersistentValueWritePolicy | PersistentValueWritePolicy_Checkpoint 0, PersistentValueWritePolicy_Finalize 1, PersistentValueWritePolicy_Disconnect 2 |
| PersistentWeaponType | PersistentWeaponType_Normal 0, PersistentWeaponType_Unlock 1 |
| PoseRestriction | prStand 0, prCrouch 1, prNone 4294967295 |
| ReadinessLevel | RLPatrol 0, RLReady 1, RLCombat 2, RLNone 3 |
| ReadinessRestriction | rrRelaxed 0, rrAlert 1, rrCombat 2, rrNone 4294967295 |
| ReadinessState | RSPatrol 0, RSReady 1, RSCombat 2 |
| ReloadBehavior | RbInCover 0, RbStationary 1, RbAnywhere 2 |
| ReloadLogic | rlWeaponSwitchCancelsUnfinishedReload 0, rlReloadUnaffectedByWeaponSwitch 1 |
| ReloadType | rtSingleBullet 0, rtMagazine 1 |
| RichPresenceContextType | RichPresenceContextType_Normal 0, RichPresenceContextType_RankedMatch 1 |
| RichPresencePropertyType | RichPresencePropertyType_Float 0, RichPresencePropertyType_Integer 1, RichPresencePropertyType_LongFloat 2, RichPresencePropertyType_LongInteger 3 |
| RouteType | RouteStop 0, RouteCircular 1, RoutePingPong 2 |
| RpgSkill | AccurateWithTracer 0, AlwaysAccurate 1, OnlyWithTracer 2 |
| ScoringBucket | ScoringBucket_General 0, ScoringBucket_Teamwork 1, ScoringBucket_Bonus 2, ScoringBucket_Squad 3, ScoringBucket_Objective 4, ScoringBucket_Award 5, ScoringBucket_Vehicle 6, ScoringBucket_Kit0 7, ScoringBucket_Kit1 8, ScoringBucket_Kit2 9, ScoringBucket_Kit3 10, ScoringBucket_LastScoringBucketItem 11 |
| ScoringBucketType | ScoringBucketType_Disabled 0, ScoringBucketType_Normal 1, ScoringBucketType_Vehicle 2, ScoringBucketType_Kit 3 |
| SoldierAnimationPose | SAPStand 0, SAPCrouch 1 |
| SoldierOnSpottedType | OSPlayer 0, OSEnemyBotsOnly 1, OSPlayerAndEnemyBots 2 |
| SoldierRole | SRCommander 0, SRSquadLeader 1 |
| SoldierSpawnInclude | SSHumanPlayer 0, SSAiPlayer 1 |
| SpecateMode | SmStatic 0, SmLookAt 1, SmFollow 2 |
| SpecateTarget | StNone 0, StAllPlayers 1, StTeamPlayers 2, StSquadPlayers 3, StOwnPlayer 4, StOwnKiller 5, StOwnPlayerAndRagdoll 6, StOwnPlayerAndVehicleRagdoll 7, StBestSquadPlayer 8 |
| SquadClass | SCInfantry 0, SCArmored 1, SCAir 2 |
| SquadId | Squad1 0, Squad2 1, Squad3 2, Squad4 3, Squad5 4, Squad6 5, Squad7 6, Squad8 7, Squad9 8, Squad10 9, Squad11 10, Squad12 11, Squad13 12, Squad14 13, Squad15 14, Squad16 15, Squad17 16, Squad18 17, Squad19 18, Squad20 19, Squad21 20, Squad22 21, Squad23 22, Squad24 23, SquadNone 24, SquadIdCount 25 |
| StatEvent | StatEvent_KillOf_X_As_Y 0, StatEvent_KillOf_X_In_Y 1, StatEvent_DeathAs_X_By_Y 2, StatEvent_DeathBy_X_In_Y 3, StatEvent_SecondsAs_X 4, StatEvent_SecondsWith_X 5, StatEvent_SecondsIn_X 6, StatEvent_Destroy_X_As_Y 7, StatEvent_Destroy_X_In_Y 8, StatEvent_Destroy_X_With_Y 9, StatEvent_CleanDestroy_X_In_Y 10, StatEvent_KillWith_X_As_Y 11, StatEvent_KillWith_X_In_Y 12, StatEvent_DeathWith_X_As_Y 13, StatEvent_DeathWith_X_In_Y 14, StatEvent_KillsWithZeroDeaths 15, StatEvent_TaggedKill_X_With_Y 16, StatEvent_TaggedDamage_X_With_Y 17, StatEvent_SquadTaggedKill_X_With_Y 18, StatEvent_SquadTaggedDamage_X_With_Y 19, StatEvent_Hit_X_With_Y 20, StatEvent_VehicleDamage 21, StatEvent_Level_X_Completed_Y_Difficulty 22, StatEvent_CleanLevel_X_Completed 23, StatEvent_Headshot_X_With_Y 24, StatEvent_HeadshotDeathAs_X_With_Y 25, StatEvent_LongRangeHeadshotAs_X_With_Y 26, StatEvent_Outcome_X_As_Y 27, StatEvent_SelfHeal_With_X 28, StatEvent_BestSquad_On_X 29, StatEvent_WonGameMode_X 30, StatEvent_ShotsFired_With_X 31, StatEvent_ShotsHit_With_X 32, StatEvent_DistanceDriven_With_X 33, StatEvent_Collectible_X 34, StatEvent_AllInSquadGainAward_X 35, StatEvent_AllInSquadKilled_X 36, StatEvent_Consumable 37, StatEvent_Heal 38, StatEvent_Revive 39, StatEvent_Resupply 40, StatEvent_KillAssist 41, StatEvent_Repair 42, StatEvent_Avenger 43, StatEvent_NemesisIsKiller 44, StatEvent_KilledNemesis 45, StatEvent_Saviour 46, StatEvent_Parachute 47, StatEvent_MultiKill 48, StatEvent_TripleKill 49, StatEvent_DoubleKill 50, StatEvent_Tag 51, StatEvent_MotionSensorAssist 52, StatEvent_SpotAssist 53, StatEvent_CrateDefendKill 54, StatEvent_CrateAttackKill 55, StatEvent_FlagDefence 56, StatEvent_FlagCaptured 57, StatEvent_FlagCapturedAssist 58, StatEvent_FlagCaptured_In_X 59, StatEvent_GenericTrigger_X_Y 60, StatEvent_BucketFilled_X 61, StatEvent_BucketUnlock_X 62, StatEvent_FirstMissionComplete 63, StatEvent_CleanMission 64, StatEvent_TreeDestroyed 65, StatEvent_HouseDestroyed 66, StatEvent_VehicleAirTime 67, StatEvent_SoldierAirTime 68, StatEvent_BaseDestroyed 69, StatEvent_LastBaseDestroyed 70, StatEvent_DefensiveKill 71, StatEvent_RankUp 72, StatEvent_UniqueAward 73, StatEvent_UniqueDogtag 74, StatEvent_WallDestroyed 75, StatEvent_FriendMeleeKill 76, StatEvent_FirstPosition 77, StatEvent_HighCard 78, StatEvent_Playgroup 79, StatEvent_SquadSpawn 80, StatEvent_Suicide 81, StatEvent_PhotoTaken 82, StatEvent_SquadAssist 83, StatEvent_SquadAvenger 84, StatEvent_SquadHeal 85, StatEvent_SquadResupply 86, StatEvent_SquadRepair 87, StatEvent_SquadRevive 88, StatEvent_SquadAttackOrderBonus 89, StatEvent_SquadDefendOrderBonus 90, StatEvent_SquadMotionSensorAssist 91, StatEvent_TwoPair 92, StatEvent_ThreeOfAKind 93, StatEvent_FiveOfAKind 94, StatEvent_FullHouse 95, StatEvent_SnakeEyes 96, StatEvent_CabDriver 97, StatEvent_Strike 98, StatEvent_UnlockToken 99, StatEvent_Outcome_X_GameMode_Y 100, StatEvent_Outcome_X_On_Y 101, StatEvent_FirstPosition_Mod_X 102, StatEvent_SpeakerDestroyed 103, StatEvent_LAST_ITEM 104 |
| StrengthType | Infantry 0, LightArmour 1, HeavyArmour 2, Helicopter 3, NumberOfStrengthTypes 4 |
| StunAllowState | SASNoChange 0, SASForbid 1, SASAllow 2 |
| SweepSelectionType | SstTargetLost 0, SstFirstTime 1 |
| TargetPosKnowledgeType | TargetPosKnown 0, TargetPosUnknown 1, TargetPosFind 2 |
| TeamAssignMode | TamEqualTeams 0, TamFullTeams 1 |
| TeamId | TeamNeutral 0, Team1 1, Team2 2, Team3 3, Team4 4, Team5 5, Team6 6, Team7 7, Team8 8, Team9 9, Team10 10, Team11 11, Team12 12, Team13 13, Team14 14, Team15 15, Team16 16, TeamIdCount 17 |
| TextChatGroup | TextChatGroup_Team 0, TextChatGroup_Squad 1, TextChatGroup_All 2, TextChatGroup_Enemy 3, TextChatGroup_Player 4, TextChatGroup_Count 5 |
| UIConsoleKeyboardType | UIConsoleKeyboardType_None 0, UIConsoleKeyboardType_RedeemCode 1, UIConsoleKeyboardType_ClanTag 2, UIConsoleKeyboardType_Generic 3, UIConsoleKeyboardType_Count 4 |
| UIMenuTrackerPage | UIMenuTrackerPage_Awards 0, UIMenuTrackerPage_MenuUnlocks 1, UIMenuTrackerPage_MenuUnlocksVietnam 2, UIMenuTrackerPage_Store 3, UIMenuTrackerPage_Count 4 |
| UIWeaponStatPresentationType | UIWeaponStatPresentation_PlainStat_Integer 0, UIWeaponStatPresentation_PlainStat_Float 1, UIWeaponStatPresentation_Accuracy 2, UIWeaponStatPresentation_Time 3 |
| VOEventType | ETLoiteringSquad 0, ETLaserDesignator 1, ETMortarStrike 2, ETTracerDartAttached 3, ETReceivedNewOrder 4, ETInvestigatingEnemy 5, ETInvestigatingEvent 6, ETRpgAimedAtPlayer 7, ETGrenadeAimedAtPlayer 8, ETThrowGrenade 9, ETVehicleHealthLow 10, ETIncomingGrenade 11, ETIdleBullChat 12, ETPlayerBadDriver 13, ETSoldierFire 14, ETProjectileImpact 15, ETProjectileImpactNearSoldier 16, ETSoldierKilled 17, ETManDown 18, ETPlayerRevived 19, ETAvengeKill 20, ETSaviourKill 21, ETVehicleKilled 22, ETAllEnemiesDead 23, ETSoldierReloading 24, ETSoldierTakingCover 25, ETSoldierLeavingCover 26, ETHeardObject 27, ETPeripherallySeenObject 28, ETSpottedNewEnemy 29, ETReacquiredEnemy 30, ETSpottedKnownEnemy 31, ETSharedNewInformation 32, ETSharedExpectedInformation 33, ETSeenExplosion 34, ETArtilleryFired 35, ETForgotSeenObject 36, ETForgotKnownObject 37, ETArmObjective 38, ETDisarmObjective 39, ETGold 40, ETLookAtPlayer 41, ETPlayerHurt 42, ETReadinessChangeAlert 43, ETEnemySpottedBehindPlayer 44, ETContact 45, ETLastManStanding 46, ETGainedReinforcements 47, ETCommentFightHarder 48, ETAlertLeavingArea 49, ETSpottedObject 50, ETSpottedExplosionPack 51, ETSpottedMine 52, ETUseMedkit 53, ETUseAmmoBox 54, ETRequestMedic 55, ETOrderBailOut 56, ETRequestRepair 57, ETPlaceAttackFocusPoint 58, ETPlaceDefendFocusPoint 59, ETKnockedOver 60, ETMeleeAttack 61, ETWonCapturePoint 62, ETLostCapturePoint 63, ETRoundWon 64, ETRoundLost 65, ETTicketsLow 66, ETReachTicketPercentage 67, ETTicketsMirror 68, ETOrderRequestMedic 69, ETOrderRequestAmmo 70, ETOrderRequestRide 71, ETOrderLetMeHeal 72, ETOrderLetMeGiveAmmo 73, ETOrderLetMeRepair 74, ETOrderSocialise 75, ETPlayerStartedFiring 76, ETPlayerStartedLockingFiring 77, ETPlayerReloadBegin 78, ETPlayerReloadLockingBegin 79, ETPlayerReloadEnd 80, ETPlayerReloadLockingEnd 81, ETPlayerFiringWhenReload 82, ETPlayerFiringWhenLockingReload 83, ETPlayerEnterEntry 84, ETRank1 85, ETRank2 86, ETRank3 87, ETRank4 88, ETRank5 89, ETRank6 90, ETRank7 91, ETRank8 92, ETRank9 93, ETRank10 94, ETRank11 95, ETRank12 96, ETRank13 97, ETRank14 98, ETRank15 99, ETRank16 100, ETRank17 101, ETRank18 102, ETRank19 103, ETRank20 104, ETRank21 105, ETRank22 106, ETRank23 107, ETRank24 108, ETRank25 109, ETRank26 110, ETSquadTakeLead 111, ETSquadLostLead 112, ETSquadKill_01 113, ETSquadKill_05 114, ETSquadKill_10 115, ETSquadKill_15 116, ETSquadKill_25 117, ETSquadKill_30 118, ETSquadKill_40 119, ETSquadKill_45 120, ETSquadKill_49 121, ETSquadKill_50 122, ETPlayerOnFire 123, ETNone 124 |
| VOListenerParamType | LptInflictor 0, LptNotInflictor 1, LptVictim 2, LptNotVictim 3 |
| VOParamGroupType | PGTRadio 0, PGTVoice 1 |
| VOParamType | PEInflictor 0, PEVictim 1, PEListener 2 |
| VOReadiness | RRelaxed 0, RCombat 1 |
| VOSource | SInflictor 0, SInflictorFriend 1, SInflictorLeader 2, SVictim 3, SVictimFriend 4, SVictimLeader 5, SPerson 6 |
| VOTeamParamType | TptInflictorTeam 0, TptVictimTeam 1 |
| VegetationDensity | All 0, EveryOther 1, EveryFourth 2, EverySixth 3, EveryEighth 4 |
| VehicleHudType | VehicleHudType_Car 0, VehicleHudType_Tank 1, VehicleHudType_Boat 2, VehicleHudType_Helicopter 3, VehicleHudType_Stationary 4, VehicleHudType_Strategic 5, VehicleHudType_UAV 6, VehicleHudType_StationaryUAV 7, VehicleHudType_Airplane 8, VehicleHudType_Dummy 9, VehicleHudType_Count 10 |
| VignetteMode | VMOff 0, VMCriticalHealth 1, VMInGameMenu 2, VMPreRound 3, VMDeath 4, VMHitIndicator 5, VMShockWave 6, VMPreset1 7, VMPreset2 8, VMHealth1 9, VMHealth2 10, VMHealth3 11, VMHealth4 12, VMRevival 13, VMFlynnCrate 14, VMEndOfRound 15, VMPlayerOnFire 16 |
| VoiceChannel | VoiceChannel_Automatic 0, VoiceChannel_Team 1, VoiceChannel_Squad 2, VoiceChannel_None 3, VoiceChannel_Count 4 |
| WarnTarget | wtWarnSoldierAndVehicle 0, wtWarnSoldier 1, wtWarnVehicle 2, wtWarnNone 3 |
| WeaponAnimBaseSetEnum | wabsRif 0, wabsHgr 1, wabsAT 2, wabsShg 3, wabsHg 4, wabsCount 5, wabsNone 6 |
| WeaponClassEnum | wcAssault 0, wcShotgun 1, wcSmg 2, wcLmg 3, wcSniper 4, wcUgl 5, wcHgr 6, wcAt 7, wcATMine 8, wcAutoInjector 9, wcC4 10, wcKnife 11, wcLaserDesignator 12, wcMedKit 13, wcMortarStrike 14, wcMotionSensor 15, wcPowerTool 16, wcTracerDart 17, wcAmmoBox 18, wcCount 19, wcNone 20 |
| WeaponOverrideValueType | OverrideValue_BulletEntity_DistributeDamageOverTime 0, OverrideValue_BulletEntity_StartDamage 1, OverrideValue_BulletEntity_EndDamage 2, OverrideValue_BulletEntity_DamageFalloffStartDistance 3, OverrideValue_BulletEntity_DamageFalloffEndDistance 4 |
| WeaponSwitchingEnum | wsPrimary1 0, wsPrimary2 1, wsGrenadeLauncher1 2, wsGrenadeLauncher2 3, wsStaticGadget 4, wsDynamicGadget1 5, wsDynamicGadget2 6, wsMeleeWeapon 7, wsHandGrenade 8, wsLastPrimary 9, wsOtherPrimary 10 |
| WheelPhysicsType | wptNormal 0, wptFollow 1, wptStatic 2 |
| ZoomTypeEnum | ztHoldButtonToZoom 0, ztPressButtonToSwitchZoomLevel 1 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: GameSharedResources enumerations 131]

### RTTI-GSR-2: The 182 value types

Status: **verified** · Variants: PC

A value type is stored inline in its owner (a DBX `complex`, TYPE-VAL-1): the gun sway's stance blocks
(`GunSwayStandData`, `GunSwayCrouchProneData`, `GunSwayStanceTransition` ...), the matchmaking rules, the scoring
buckets, the VO parameters, the UI's grids and entries, the input maps' slots. The table gives each one's size and
its members' offsets inside it.

<!-- typeinfo:gsr-values -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AchievementData | value type | - | 12 | Award: AwardData @0, TrophyId: Int32 @4, AchievementId: Int32 @8 |
| ActionSuppressor | value type | - | 8 | SuppressingValue: Float32 @0, ActionToSuppress: EntryInputActionEnum @4 |
| AdvertisementZoneData | value type | - | 28 | Members: AdvertisementZoneMember[] @0, Identifier: String @16 |
| AdvertisementZoneMember | value type | - | 24 | Filename: String @0, Identifier: String @12 |
| AimingConstraintsData | value type | - | 16 | MinPitch: Float32 @0, MaxPitch: Float32 @4, MinYaw: Float32 @8, MaxYaw: Float32 @12 |
| AimingPoseData | value type | - | 24 | AimSteadiness: Float32 @0, SpeedMultiplier: Float32 @4, RecoilMultiplier: Float32 @8, MinimumPitch: Float32 @12, MaximumPitch: Float32 @16, TargetingFov: Float32 @20 |
| AmbientModelWaypointSplineData | value type | - | 32 | Gradient: Vec3 @0, StepCount: Float32 @16, DistanceFromPrevious: Float32 @20 |
| AmmoConfigData | value type | - | 36 | AutoReplenishDelay: Float32 @0, AmmoPickupMaxAmount: Uint32 @4, AmmoBoxPickupAmount: Int32 @8, AmmoBoxPickupRate: Uint32 @12, NumberOfMagazines: Int32 @16, MagazineCapacity: Int32 @20, TraceFrequency: Uint32 @24, AmmoPickupMinAmount: Uint32 @28, AutoReplenishMagazine: Boolean @32 |
| AmmoResupplyData | value type | - | 16 | Delay: Float32 @0, UpdateFrequency: Uint32 @4, Radius: Float32 @8, Ammo: Uint32 @12 |
| AnimationSoundCategoryData | value type | - | 28 | Name: String @0, AnimSound: AnimationSoundData[] @12 |
| AnimationSoundData | value type | - | 8 | Sound: SoundAsset @0, Animation: BaseAnimationAsset @4 |
| AnimationStateData | value type | - | 16 | Aiming: AimState @0, Movement: MovementState @4, Pose: SoldierAnimationPose @8, InCover: CoverState @12 |
| AntiSpamConfig | value type | - | 8 | WarningSound: WaveAsset @0, TimeBetweenOrderExecution: Float32 @4 |
| AreaValueTeam | value type | - | 8 | Team: TeamId @0, AreaValue: Int32 @4 |
| AutoAimData | value type | - | 80 | AutoAimInnerBoxOffset: Vec3 @0, AutoAimInnerBoxExtends: Vec3 @16, AutoAimOuterBoxOffset: Vec3 @32, AutoAimOuterBoxExtends: Vec3 @48, PoseType: CharacterEntityPoseEnum @64 |
| AwardStarNameInstance | value type | - | 16 | Category: StatsCategoryData @0, Name: String @4 |
| BehaviourMappingData | value type | - | 24 | Plan: String @0, Behaviour: String @12 |
| BehaviourSetting | value type | - | 16 | DefaultModifier: Float32 @0, Behaviour: String @4 |
| BinarySetting | value type | - | 16 | Name: String @0, UserExposed: Boolean @12 |
| BlurEffectData | value type | - | 24 | BulletStrength: Float32 @0, ExplosionFalloffSpeed: Float32 @4, BulletFalloffSpeed: Float32 @8, DispersionStrength: Float32 @12, DispersionThreshhold: Float32 @16, ExplosionStrength: Float32 @20 |
| BoltActionData | value type | - | 12 | BoltActionTime: Float32 @0, BoltActionDelay: Float32 @4, HoldBoltActionUntilZoomRelease: Boolean @8, HoldBoltActionUntilFireRelease: Boolean @9 |
| BoneCollisionData | value type | - | 80 | CapsuleOffset: Vec3 @0, DebugDrawColor: Vec4 @16, CapsuleLength: Float32 @32, CapsuleRadius: Float32 @36, BoneAxis: Int32 @40, BoneName: String @44, Material: String @56 |
| CameraImpulseCurveData | value type | - | 36 | MaxAngle: Float32 @0, Damping: Float32 @4, AverageFilterSize: Uint32 @8, MaxActiveImpulses: Uint32 @12, LeadInTime: Float32 @16, MaxDuration: Float32 @20, MinDuration: Float32 @24, MaxFrequency: Float32 @28, MinFrequency: Float32 @32 |
| CameraImpulseShakeData | value type | - | 108 | RollCurve: CameraImpulseCurveData @0, PitchCurve: CameraImpulseCurveData @36, YawCurve: CameraImpulseCurveData @72 |
| CameraLeapData | value type | - | 80 | Transform: LinearTransform @0, FovModifier: Float32 @64, Time: Float32 @68, OnActivate: Boolean @72 |
| CollectibleWeapon | value type | - | 20 | Persistence: PersistentWeapon @0, Texture: TextureAsset @4, Sid: String @8 |
| CollectibleWeaponData | value type | - | 20 | Weapons: CollectibleWeapon[] @0, Sound: WaveAsset @16 |
| ColorTransformData | value type | - | 160 | Saturation: Vec3 @0, ContrastFrequency: Vec3 @16, SaturationFrequency: Vec3 @32, SaturationModifier: Vec3 @48, ContrastModifier: Vec3 @64, BrightnessModifier: Vec3 @80, Brightness: Vec3 @96, Contrast: Vec3 @112, BrightnessFrequency: Vec3 @128, Hue: Float32 @144 |
| CombatAreaCamera | value type | - | 80 | Data: MinimapData @0 |
| ContinueWeaponAmmoData | value type | - | 8 | WeaponSlot: Int32 @0, MinMags: Int32 @4 |
| CreditsCategory | value type | - | 28 | Headlines: CreditsHeadline[] @0, Sid: String @16 |
| CreditsData | value type | - | 40 | Images: TextureAsset[] @0, Categories: CreditsCategory[] @20, NumRowsPerPage: Int32 @36 |
| CreditsEntity | value type | - | 24 | Name: String @0, Title: String @12 |
| CreditsHeadline | value type | - | 28 | Entities: CreditsEntity[] @0, Sid: String @16 |
| CriteriaAward | value type | - | 24 | SID: String @0, Count: Uint32 @12, Award: AwardData @16, Combined: Boolean @20 |
| CriteriaStarCategoryInstance | value type | - | 20 | ParamY: StatsCategoryData @0, ParamX: StatsCategoryData @4, Name: String @8 |
| CutsceneObjectAnimationData | value type | - | 20 | BlendInTime: Float32 @0, BlendOutTime: Float32 @4, NextAnimationIndex: Int32 @8, Animation: AnimationAsset @12, IsLooping: Boolean @16 |
| DestructibleLocalPathfindingPart | value type | - | 48 | BoundingBoxForwardXZ: Vec2 @0, BoundingBoxExtentsXZ: Vec2 @8, CenterPos: Vec3 @16, Id: DestructiblePartId @32 |
| DestructiblePartId | value type | - | 8 | PartIndex: Uint32 @0, ObjectEnumeration: Uint32 @4 |
| DynamicWeaponPickupSlotData | value type | - | 12 | LinkedToWeaponSlot: Int32 @0, AltWeaponSlot: Int32 @4, WeaponSlot: Uint32 @8 |
| EdgeModelInstance | value type | - | 96 | Normal: Vec3 @0, Transform: LinearTransform @16, InstanceSetIndex: Uint32 @80, EdgeModelIndex: Uint32 @84 |
| EdgeModelLinkData | value type | - | 20 | Link: PartLinkData @0, Instances: EdgeModelInstance[] @4 |
| EdgeModelMultiLinkData | value type | - | 32 | PartComponentIndices: Uint32[] @0, Instances: EdgeModelInstance[] @16 |
| EdgeModelPlacementSettings | value type | - | 56 | PlacementType: EdgeModelType @0, Probability: Int32 @4, InwardOffsets: FloatRange[] @8, HorizontalAngles: FloatRange[] @24, VerticalAngles: FloatRange[] @40 |
| EdgeModelVolumeGroup | value type | - | 28 | Groups: String[] @0, OutputFileName: String @16 |
| EdgeModelVolumeMapItemData | value type | - | 32 | VolumeGroupName: String @0, Meshes: EdgeModelMeshData[] @12 |
| EditableActionMap | value type | - | 48 | ActionMap: EntryInputActionMapsData @0, ConfigurationLayout: EditableActions @4, Id: String @24, NameId: String @36 |
| EditableActions | value type | - | 20 | Actions: EditableAction[] @0 |
| EloParameters | value type | - | 80 | KCompetitor: FunctionPoint[] @0, Expected: ExpectedFunctionPoint[] @16, KNewbie: FunctionPoint[] @32, KWinner: FunctionPoint[] @48, KLoser: FunctionPoint[] @64 |
| Entitlement | value type | - | 80 | EntitlementTag: String @0, InternalName: String @12, GroupName: String @24, ProductId: String @36, ProductCatalog: String @48, License: String @60, CountThreshold: Uint32 @72, VeteranEntitlement: Boolean @76, AutomaticallyGranted: Boolean @77, GrantByCode: Boolean @78, UsOnly: Boolean @79 |
| EntryComponentHudData | value type | - | 8 | Index: Int32 @0, Visible: Boolean @4, Frustum: Boolean @5 |
| EulaTexture | value type | - | 8 | Image: TextureAsset @0, Language: LanguageFormat @4 |
| EventScaleData | value type | - | 8 | Scale: Float32 @0, Event: StatEvent @4 |
| ExpectedFunctionPoint | value type | - | 8 | YValue: Float32 @0, XValue: Float32 @4 |
| ExtraMoviesConfig | value type | - | 96 | Trailers: UIMovieAsset[] @0, SubRipLinks: UISubRipLink[] @20, OtherGames: UIMovieAsset[] @36, TutorialMovies: UIMovieAsset[] @56, SPCutScenes: UIMovieAsset[] @76 |
| FacialSoundMapData | value type | - | 8 | AnimIndex: Uint32 @0, SoundAsset: Asset @4 |
| FakeAnimatedSoldierCollisionPose | value type | - | 96 | MaxAbsoluteSpeed: Float32 @0, MinAbsoluteSpeed: Float32 @4, AimingOption: AimOption @8, AimLeftRightBlendCurve: FakeCollisionPoseBlendCurve @12, AimUpDownBlendCurve: FakeCollisionPoseBlendCurve @28, MaxForwardSpeed: Float32 @44, Description: String @48, Bones: FakeBone[] @60, Name: String @76, MinForwardSpeed: Float32 @88, UseAbsoluteForwardSpeed: Boolean @92, Stand: Boolean @93 |
| FakeBone | value type | - | 64 | Forward: Vec3 @0, Position: Vec3 @16, Radius: Float32 @32, Angle: Float32 @36, Limb: FakePoseLimbs @40, Length: Float32 @44, DrawWhenDebugging: Boolean @48 |
| FakeCollisionPoseBlendCurve | value type | - | 16 | Point3: Float32 @0, Point4: Float32 @4, Point1: Float32 @8, Point2: Float32 @12 |
| FakePoseLimbInfo | value type | - | 28 | BoneName: String @0, MaterialName: String @12, LimbIndex: FakePoseLimbs @24 |
| FireEffectData | value type | - | 80 | Rotation: Vec3 @0, ZoomRotation: Vec3 @16, ZoomOffset: Vec3 @32, Offset: Vec3 @48, Effect: EffectAsset @64, DisableDuringZoom: Boolean @68, UseZoomRotation: Boolean @69, UseZoomOffset: Boolean @70 |
| FireLogicData | value type | - | 140 | PostReloadSequenceTime: Float32 @0, ReloadThreshold: Float32 @4, ReloadDelay: Float32 @8, ReloadTime: Float32 @12, PreFireDelay: Float32 @16, ReloadType: ReloadType @20, FireLogicType: FireLogicType @24, ZoomBeforeFireDelay: Float32 @28, ReloadLogic: ReloadLogic @32, HoldAndRelease: HoldAndReleaseData @36, FireInputAction: EntryInputActionEnum @64, BoltAction: BoltActionData @68, Recoil: RecoilData @80, OrderInputAction: EntryInputActionEnum @116, RateOfFire: Float32 @120, ClientFireRateMultiplier: Float32 @124, ReloadInputAction: EntryInputActionEnum @128, TriggerPullWeight: Float32 @132, WaitForTriggerRelease: Boolean @136, WaitForZoomRelease: Boolean @137, HoldReloadUntilFireRelease: Boolean @138, HoldReloadUntilZoomRelease: Boolean @139 |
| FiringDispersionData | value type | - | 16 | IncreasePerShot: Float32 @0, DecreasePerSecond: Float32 @4, MinAngle: Float32 @8, MaxAngle: Float32 @12 |
| FloatRange | value type | - | 12 | Probability: Uint32 @0, Max: Float32 @4, Min: Float32 @8 |
| FloatSetting | value type | - | 20 | Value: Float32 @0, Name: String @4, UserExposed: Boolean @16 |
| FootStepTrigger | value type | - | 4 | Time: Float32 @0 |
| FrameEffectState | value type | - | 32 | Opacity: Vec4 @0, CenterAlpha: Float32 @16, Size: Float32 @20 |
| FunctionPoint | value type | - | 8 | YValue: Float32 @0, XValue: Float32 @4 |
| GameModeObjective | value type | - | 24 | Defender: String @0, Attacker: String @12 |
| GameSettingsCategory | value type | - | 68 | Strings: StringSetting[] @0, Binaries: BinarySetting[] @16, Ints: IntSetting[] @32, Category: GameSettingsType @48, Floats: FloatSetting[] @52 |
| GameTipData | value type | - | 24 | Header: String @0, Text: String @12 |
| GroupMemberInstanceSpawnData | value type | - | 8 | MeshShaderSet: Asset @0, Enumeration: Uint32 @4 |
| GunSwayBaseData | value type | - | 60 | GunSwayLag: GunSwayLagData @0, Recoil: GunSwayRecoilData @20, BaseValue: GunSwayDispersionData @40 |
| GunSwayBaseMoveData | value type | - | 80 | Recoil: GunSwayRecoilData @0, GunSwayLag: GunSwayLagData @20, BaseValue: GunSwayDispersionData @40, Moving: GunSwayDispersionData @60 |
| GunSwayBaseMoveJumpData | value type | - | 140 | VaultingMediumObject: GunSwayDispersionData @0, Recoil: GunSwayRecoilData @20, GunSwayLag: GunSwayLagData @40, VaultingSmallObject: GunSwayDispersionData @60, BaseValue: GunSwayDispersionData @80, Moving: GunSwayDispersionData @100, Jumping: GunSwayDispersionData @120 |
| GunSwayCrouchProneData | value type | - | 160 | Zoom: GunSwayBaseMoveData @0, NoZoom: GunSwayBaseMoveData @80 |
| GunSwayDispersionData | value type | - | 20 | IncreasePerShot: Float32 @0, DecreasePerSecond: Float32 @4, MinAngle: Float32 @8, MaxAngle: Float32 @12, UseAimSpecialisation: Boolean @16 |
| GunSwayDispersionModData | value type | - | 16 | IncreasePerShotModifier: Float32 @0, DecreasePerSecondModifier: Float32 @4, MinAngleModifier: Float32 @8, MaxAngleModifier: Float32 @12 |
| GunSwayLagData | value type | - | 20 | RotatePitchModifier: Float32 @0, ReleaseModifier: Float32 @4, RotateYawModifier: Float32 @8, MoveStrafeModifier: Float32 @12, MoveForwardModifier: Float32 @16 |
| GunSwayRecoilData | value type | - | 20 | RecoilAngleIncPerShot: Float32 @0, RecoilAmplitudeDecreaseFactor: Float32 @4, RecoilAngleMax: Float32 @8, RecoilAmplitudeMax: Float32 @12, RecoilAmplitudeIncPerShot: Float32 @16 |
| GunSwayStanceTransition | value type | - | 24 | CoolDown: Float32 @0, MaxPenaltyValue: GunSwayDispersionData @4 |
| GunSwayStanceZoomModifierData | value type | - | 32 | LagYawMod: Float32 @0, LagPitchMod: Float32 @4, RecoilAngleMod: Float32 @8, DispersionMod: GunSwayDispersionModData @12, RecoilMagnitudeMod: Float32 @28 |
| GunSwayStandData | value type | - | 280 | Zoom: GunSwayBaseMoveJumpData @0, NoZoom: GunSwayBaseMoveJumpData @140 |
| HealerData | value type | - | 16 | HealingSpeed: Float32 @0, Delay: Float32 @4, Radius: Float32 @8, Health: Float32 @12 |
| HealthVignetteState | value type | - | 8 | Mode: VignetteMode @0, Health: Float32 @4 |
| HidableSoldierMeshWeaponPart | value type | - | 20 | BoneName: String @0, WeaponMesh: SkinnedMeshAsset @12, HideInVehicleEntries: Boolean @16, HideAlways: Boolean @17 |
| HoldAndReleaseData | value type | - | 28 | PowerIncreasePerSecond: Float32 @0, Delay: Float32 @4, KilledHoldingPowerModifier: Float32 @8, MaxHoldTime: Float32 @12, MinPowerModifier: Float32 @16, MaxPowerModifier: Float32 @20, ForceFireWhenKilledHolding: Boolean @24 |
| HudData | value type | - | 80 | HintAction1: EntryInputActionEnum @0, WeaponClass: String @4, HintAction2: EntryInputActionEnum @16, HintLine2: String @20, HintLine1: String @32, CrosshairTypeId: String @44, CrosshairScaleMax: Float32 @56, CrosshairScaleMin: Float32 @60, CrosshairOpacityMin: Float32 @64, CrosshairOpacityModifier: Float32 @68, CrosshairOpacityMax: Float32 @72, IsSpecialItem: Boolean @76, HideCrosshairWhenAimOnFriend: Boolean @77, InfiniteAmmo: Boolean @78 |
| HudImpactData | value type | - | 20 | MaxTimeVisible: Float32 @0, MinTimeVisible: Float32 @4, MaxHealth: Float32 @8, MinHealth: Float32 @12, Enable: Boolean @16 |
| HudRenderConfig | value type | - | 48 | GlowColor: Vec3 @0, GlowOpacity: Float32 @16, FontSize: Uint32 @20, FontName: String @24, FontStyle: UIFontStyle @36, Glow: Boolean @40 |
| InputSuppressionData | value type | - | 16 | SuppressVehicleInput: ActionSuppressor[] @0 |
| IntSetting | value type | - | 20 | Value: Int32 @0, Name: String @4, UserExposed: Boolean @16 |
| IntroMovieData | value type | - | 20 | Movies: UIMovieAsset[] @0 |
| KitImageData | value type | - | 20 | Texture: TextureAsset @0, KitCategory: String @4, Team: TeamId @16 |
| LevelDescription | value type | - | 364 | LoadingMovieDimensions: Vec2 @0, LoadingMovieTopLeft: Vec2 @8, LevelMemoryUsage: LevelMemoryUsageData @16, GameMode: GameModeData @32, StartDelayTime: Uint32 @36, MinimapCameraOffset: MinimapCameraOffsetData @40, Consumables: String[] @52, Advertisement: AdvertisementZoneData @68, RichPresence: RichPresenceConnection @96, Collectibles: String[] @100, Revision: Uint32 @116, Licenses: String @120, MatchmakingModID: String @132, LevelRoundTimeStatName: String @144, FactionTeam2: FactionId @156, FactionTeam1: FactionId @160, KitImages: KitImageData[] @164, GameTips: GameTipData[] @180, AchivementHardDate: String @196, LoadingMovieLoopEndFrame: Uint32 @208, LoadingMovieLoopBeginFrame: Uint32 @212, LoadingTime: LoadingTimeData @216, AttackerTeam: TeamId @228, Description: String @232, Name: String @244, LoadingMovie: BinkMemoryAsset @256, LoadingScreens: LoadingScreenData[] @260, Music: WaveAsset @276, AchivementMediumCompleted: String @280, AchivementEasyDate: String @292, AchivementHardCompleted: String @304, AchivementMediumDate: String @316, SoundFX: LocalizedSoundFX[] @328, MinimapTexture: TextureAsset @344, AchivementEasyCompleted: String @348, WaitForButtonBeforeStarting: Boolean @360, GatherCollectibles: Boolean @361, KickIfLicenseMissing: Boolean @362 |
| LevelMemoryUsageData | value type | - | 16 | CutsceneSmallHeapSize: Uint32 @0, CutsceneLargeHeapSize: Uint32 @4, StaticAnimationsInBytes: Uint32 @8, WeaponAsyncHeapSize: Uint32 @12 |
| LoadingScreenData | value type | - | 16 | Texture: TextureAsset @0, Text: String @4 |
| LoadingTimeData | value type | - | 12 | Ps3Time: Float32 @0, XenonTime: Float32 @4, Win32Time: Float32 @8 |
| LocalizedSoundFX | value type | - | 20 | Sounds: String[] @0, Language: LanguageFormat @16 |
| MPModeData | value type | - | 4 | VehiclePoints: Int32 @0 |
| MatchMakingFitThreshold | value type | - | 8 | MinimumFitScore: Uint32 @0, Seconds: Uint32 @4 |
| MatchMakingMapLink | value type | - | 8 | Next: MatchMakingMap @0, Current: MatchMakingMap @4 |
| MatchMakingPoolOverride | value type | - | 28 | PoolOverride: Uint32 @0, Value: String @4, Filter: String @16 |
| MatchMakingPoolPlayerCountThreshold | value type | - | 8 | TargetPlayerCount: Uint32 @0, Seconds: Uint32 @4 |
| MaterialBigWorldMapData | value type | - | 32 | BigWorld: BigWorldSettingsAsset[] @0, MaterialName: String @20 |
| MaterialSoundMapData | value type | - | 16 | Sound: SoundAsset @0, MaterialName: String @4 |
| MinimapCameraOffsetData | value type | - | 12 | Position: Vec2 @0, CameraFov: Float32 @8 |
| MinimapSettings | value type | - | 192 | ObjectColor: Vec3 @0, TerrainColor: Vec3 @16, RoadColor: Vec3 @32, VegetationColor: Vec3 @48, WaterColor: Vec3 @64, AltitudeCurveDistance: Int32 @80, TextureResolution: Int32 @84, WaterHeightModifier: Float32 @88, RoadHeightOverTerrain: Float32 @92, ObjectsToReplace: MeshAsset[] @96, ObjectBoundingBoxMesh: MeshAsset @116, MinObjectBoundingRadius: Float32 @120, MinimapObjects: MeshAsset[] @124, VegetationDensity: VegetationDensity @144, MinimapVegetationObjects: MeshAsset[] @148, VegetationObjectsToReplace: MeshAsset[] @168, GenerateAllLayers: Boolean @188, GenerateMinimap: Boolean @189, GenerateRiverMesh: Boolean @190, GenerateLakeMesh: Boolean @191 |
| OnlineProviderConfiguration | value type | - | 104 | Password: String @0, Username: String @12, ServiceId: String @24, TheaterPort: Uint32 @36, Port: Uint32 @40, PacketSize: Uint32 @44, Partition: String @48, Client: String @60, Platform: OnlineProviderPlatform @72, Version: String @76, SKU: String @88, DefaultUnsecure: Boolean @100, IsServer: Boolean @101 |
| OrderEventSettings | value type | - | 16 | Immortal: EventSendOption @0, VoiceOvers: EventSendOption @4, Sensing: EventSendOption @8, Target: EventSendOption @12 |
| OrientedBoundingBox | value type | - | 112 | BoundingBox: AxisAlignedBox @0, Transform: LinearTransform @32, Name: String @96 |
| OverHeatData | value type | - | 112 | OverHeatThreshold: Float32 @0, OverHeatEffect: FireEffectData @16, OverHeatPenaltyTime: Float32 @96, HeatPerBullet: Float32 @100, HeatDropPerSecond: Float32 @104 |
| PickupSettingsData | value type | - | 32 | MeshRenderRotation: Vec3 @0, MeshRenderOffset: Vec3 @16 |
| PoseConstraintsData | value type | - | 3 | PronePose: Boolean @0, CrouchPose: Boolean @1, StandPose: Boolean @2 |
| RagdollImpulse | value type | - | 48 | RandomMaxImpulse: Vec3 @0, RandomMinImpulse: Vec3 @16, BoneName: String @32 |
| RankLevelData | value type | - | 60 | SoundName: String @0, Sound: WaveAsset @12, VOEvent: VOEventType @16, VO: DialogLineNode @20, ImageName: String @24, PointsNeeded: Uint32 @36, RankName: String @40, Image: TextureAsset @52, Unlock: PersistentWeapon @56 |
| ReadinessAnimations | value type | - | 2 | Alert: Boolean @0, Relaxed: Boolean @1 |
| ReadinessLevelCharacteristics | value type | - | 12 | ReductionTime: Float32 @0, MaxReactionTime: Float32 @4, MinReactionTime: Float32 @8 |
| ReadinessLevels | value type | - | 36 | Combat: ReadinessLevelCharacteristics @0, Ready: ReadinessLevelCharacteristics @12, Patrol: ReadinessLevelCharacteristics @24 |
| RecoilData | value type | - | 36 | MinRecoilAngleZ: Float32 @0, MaxRecoilAngleZ: Float32 @4, MinRecoilFov: Float32 @8, MaxRecoilFov: Float32 @12, MinRecoilAngleX: Float32 @16, MaxRecoilAngleX: Float32 @20, MinRecoilAngleY: Float32 @24, MaxRecoilAngleY: Float32 @28, RecoilFollowsDispersion: Boolean @32 |
| RichPresenceContextPair | value type | - | 12 | ContextType: RichPresenceContextType @0, ValueIndex: Uint32 @4, ContextIndex: Uint32 @8 |
| RichPresencePropertyPair | value type | - | 4 | Property: RichPresenceProperty @0 |
| RotorModelData | value type | - | 16 | MeshName: String @0, RotationRpm: Float32 @12 |
| RumbleFiringData | value type | - | 12 | RumbleDuration: Float32 @0, HighRumble: Float32 @4, LowRumble: Float32 @8 |
| ScoreEntry | value type | - | 20 | Bucket: ScoringBucket @0, Score: Float32 @4, UIString: String @8 |
| ScoringBucketConfiguration | value type | - | 16 | Entries: ScoringBucketConfigurationEntry[] @0 |
| ScoringBucketConfigurationEntry | value type | - | 40 | Name: String @0, Unlocks: ScoringBucketUnlockData[] @12, Bucket: ScoringBucket @28, BucketType: ScoringBucketType @32, RoundScore: Boolean @36, AddToEntry: Boolean @37, GlobalScore: Boolean @38 |
| ScoringBucketUnlockData | value type | - | 8 | Unlock: PersistentWeapon @0, PointsNeeded: Uint32 @4 |
| SensingLimits | value type | - | 28 | PeripheralViewAngle: Float32 @0, PeripheralViewDistance: Float32 @4, PeripheralMovementCutoff: Float32 @8, ViewDistance: Float32 @12, HearingDistance: Float32 @16, HearingCutoff: Float32 @20, ViewAngle: Float32 @24 |
| ShotConfigData | value type | - | 80 | InitialSpeed: Vec3 @0, InitialDirection: Vec3 @16, InitialPosition: Vec3 @32, SpawnDelay: Float32 @48, NumberOfBulletsPerShell: Uint32 @52, NumberOfBulletsPerShot: Uint32 @56, InheritWeaponSpeedAmount: Float32 @60, MuzzleExplosion: ExplosionEntityData @64, ProjectileData: ProjectileEntityData @68, ActiveForceSpawnToCamera: Boolean @72, ForceSpawnToCamera: Boolean @73, RelativeTargetAiming: Boolean @74 |
| SoldierBodyRotationData | value type | - | 20 | ReturnSpeed: Float32 @0, LowerBodyDelayUntilRotateIsActive: Float32 @4, MaxAngle: Float32 @8, MaxAngleAiming: Float32 @12, UseUpperBodyRotation: Boolean @16 |
| SoldierHeadCollisionPoseData | value type | - | 32 | MovingForwardOffset: Vec3 @0, IdleOffset: Vec3 @16 |
| SoldierMeshData | value type | - | 12 | RandomGroup: Int32 @0, MeshShaderSet: MeshShaderSetAsset @4, Mesh: SkinnedMeshAsset @8 |
| SoldierWeaponDispersion | value type | - | 20 | MoveZoomedDispersionAngle: Float32 @0, DecreasePerSecond: Float32 @4, MoveDispersionAngle: Float32 @8, JumpDispersionAngle: Float32 @12, ProneTransitionDispersionAngle: Float32 @16 |
| SoldierWeaponUnlockData | value type | - | 12 | WeaponSlot: Uint32 @0, Weapon: SoldierWeaponAsset @4, IsBundledWithSelectable: Boolean @8, IsSelectable: Boolean @9 |
| SoundTrigInfo | value type | - | 20 | Swim: Float32 @0, Climb: Float32 @4, Prone: Float32 @8, Stand: Float32 @12, Crouch: Float32 @16 |
| StaticBone | value type | - | 80 | Transform: LinearTransform @0, BoneId: Int32 @64 |
| StaticJointData | value type | - | 32 | Translation: Vec3 @0, BoneIndex: Uint32 @16 |
| StaticModelPartToBangerParts | value type | - | 16 | BangerPartIndices: Uint32[] @0 |
| Strengths | value type | - | 16 | HeavyArmour: Float32 @0, Helicopter: Float32 @4, Infantry: Float32 @8, LightArmour: Float32 @12 |
| StringSetting | value type | - | 28 | Value: String @0, Name: String @12, UserExposed: Boolean @24 |
| SubsystemTimingConfigData | value type | - | 40 | GoalUpdates: Float32 @0, Evaluation: Float32 @4, BehaviourSelection: Float32 @8, SquadUpdate: Float32 @12, BehaviourExecution: Float32 @16, Traversals: Float32 @20, Pathfinding: Float32 @24, AreaDistribution: Float32 @28, Sensing: Float32 @32, DynamicObjectEnvironment: Float32 @36 |
| TeamVehicleAsset | value type | - | 8 | Team: TeamId @0, Vehicle: VehicleAsset @4 |
| TicketCountPercentage | value type | - | 12 | VOEvent: VOEventType @0, TicketLossPerMin: Int32 @4, MaxPercentOfTicketsLeft: Int32 @8 |
| TurnEffectData | value type | - | 32 | BeginTime: Float32 @0, SafeTime: Float32 @4, EndTime: Float32 @8, HoldTime: Float32 @12, MaxPitchAngle: Float32 @16, MaxRollAngle: Float32 @20, VelocityThreshhold: Float32 @24, YawVelocityThreshhold: Float32 @28 |
| UIAwardGridData | value type | - | 32 | GroupHeaders: UIGridHeader[] @0, Awards: UIAwardGridEntry[] @16 |
| UIAwardGridEntry | value type | - | 12 | Award: AwardData @0, GroupPosition: UIGridPosition @4 |
| UICollectibleEntry | value type | - | 44 | LevelName: String @0, GroupPosition: UIGridPosition @12, PersistenceName: String @20, WeaponName: String @32 |
| UICollectiblesData | value type | - | 32 | GroupHeaders: UIGridHeader[] @0, Entries: UICollectibleEntry[] @16 |
| UIControlInputTypeInfo | value type | - | 72 | LayoutEntriesXenon: UIInputNodeDescription[] @0, DescriptionSID: String @20, LayoutEntries: UIInputNodeDescription[] @32, LayoutEntriesPS3: UIInputNodeDescription[] @52 |
| UIFeatureHintData | value type | - | 40 | BodySID: String @0, Texture: TextureAsset @12, PageID: String @16, HeaderSID: String @28 |
| UIGridHeader | value type | - | 20 | Right: Int32 @0, Left: Int32 @4, HeaderSID: String @8 |
| UIGridPosition | value type | - | 8 | y: Int32 @0, x: Int32 @4 |
| UIInputSchemeInfo | value type | - | 32 | DescriptionSID: String @0, InputTypes: UIControlInputTypeInfo[] @12, ActionMapSettingsScheme: Int32 @28 |
| UIKillMsgSIDMapping | value type | - | 28 | PresentationSID: String @0, WeaponName: String[] @12 |
| UIMenuNameTranslation | value type | - | 24 | NameToUse: String @0, FlashName: String @12 |
| UIMenuTransitionConfig | value type | - | 36 | MenuNameTranslations: UIMenuNameTranslation[] @0, FallbackMovie: UIMovieAsset @16, Transitions: UIMenuTransitionData[] @20 |
| UIMenuTransitionData | value type | - | 36 | FadeMovie: UIMovieAsset @0, ToMovie: UIMovieAsset @4, FromMovie: UIMovieAsset @8, FromName: String @12, ToName: String @24 |
| UISPMissionPresentationConfig | value type | - | 52 | Position: Vec2 @0, ObjectiveMapMarkerAnimationThreshold: Float32 @8, ObjectiveAnimationTime: Float32 @12, TextEffectMinDelay: Float32 @16, ObjectiveCompleteAnimationTime: Float32 @20, TextEffectMaxDelay: Float32 @24, ObjectiveDisplayTime: Float32 @28, ObjectiveIndent: Float32 @32, ObjectiveHeight: Float32 @36, ObjectiveLineOffsetY: Float32 @40, MissionAnimationTime: Float32 @44, MissionDisplayTime: Float32 @48 |
| UISubRipEntry | value type | - | 20 | Sid: String @0, EndTime: Float32 @12, StartTime: Float32 @16 |
| UISubRipLink | value type | - | 16 | MovieName: String @0, SubRip: UISubRipAsset @12 |
| UIUnlockEntry | value type | - | 60 | RequiredAwardCode: String @0, RequiredAwardCount: Int32 @12, RequiredRank: Int32 @16, Children: UIUnlockEntry[] @20, Persistence: PersistentWeapon @36, WeaponName: String @40, GroupPosition: UIGridPosition @52 |
| UIUnlocksData | value type | - | 32 | GroupHeaders: UIGridHeader[] @0, Entries: UIUnlockEntry[] @16 |
| UIVehicleStatEntry | value type | - | 72 | StarAwardSuffix: String @0, Texture: TextureAsset @12, GroupPosition: UIGridPosition @16, ClassSID: String @24, VehicleName: String @36, VehicleNameSID: String @48, DescriptionSID: String @60 |
| UIVehicleStatsData | value type | - | 32 | GroupHeaders: UIGridHeader[] @0, Entries: UIVehicleStatEntry[] @16 |
| UIWeaponData | value type | - | 124 | Damage: Float32 @0, Accuracy: Float32 @4, CollectiblesDescriptionSID: String @8, Handling: Float32 @20, ProficiencyStatSID: String @24, ClipSize: Float32 @36, RateOfFire: Float32 @40, Texture: TextureAsset @44, UnlockID: String @48, WeaponName: String @60, NameSID: String @72, StatDescriptionSID: String @84, Abbreviation: String @96, DescriptionSID: String @108, IsSpecialItem: Boolean @120 |
| UIWeaponStatsEntry | value type | - | 48 | PresentationType: UIWeaponStatPresentationType @0, ArgXPersistenceIDOverride: String @4, ArgYPersistenceIDOverride: String @16, StatSID: String @28, StatEvent: StatEvent @40, ArgYPersistenceID: Boolean @44, ArgXPersistenceID: Boolean @45 |
| UIWeaponStatsPresentation | value type | - | 44 | Stats: UIWeaponStatsEntry[] @0, ProficiencyHeaderSID: String @16, PersistenceId: String[] @28 |
| UrgencyModifierData | value type | - | 16 | Modifier: Float32 @0, Name: String @4 |
| VehicleHealthZoneData | value type | - | 48 | EffectPosition: Vec3 @0, MinDamageAngle: Float32 @16, DamageAngleMultiplier: Float32 @20, Effect: EffectAsset @24, MaxHealth: Float32 @28, UseDamageAngleCalculation: Boolean @32 |
| VehicleHudData | value type | - | 8 | Texture: TextureAsset @0, VehicleType: VehicleHudType @4 |
| VehicleRumbleData | value type | - | 44 | RumbleImpulseClamp: Float32 @0, RumbleImpulseDuration: Float32 @4, EngineStartLowRumbleModifier: Float32 @8, EngineStartRumbleDuration: Float32 @12, EngineStartHighRumbleModifier: Float32 @16, HighRumbleImpulseModifier: Float32 @20, RumbleRadius: Float32 @24, SpeedThreshold: Float32 @28, LowRumbleModifier: Float32 @32, LowRumbleImpulseModifier: Float32 @36, HighRumbleModifier: Float32 @40 |
| VignetteData | value type | - | 208 | Scale: Vec2 @0, Color: Vec3 @16, ColorTransform: ColorTransformData @32, BlurAmount: Float32 @192, Opacity: Float32 @196, Exponent: Float32 @200, Mode: VignetteMode @204 |
| WeaponLagEffectForceData | value type | - | 32 | RotationForce: Vec3 @0, OffsetForce: Vec3 @16 |
| WeaponLagEffectSpringData | value type | - | 8 | Damping: Float32 @0, Constant: Float32 @4 |
| WeaponLagEffectSpringVector | value type | - | 24 | SpringZ: WeaponLagEffectSpringData @0, SpringY: WeaponLagEffectSpringData @8, SpringX: WeaponLagEffectSpringData @16 |
| WeaponOverrideValue | value type | - | 8 | Value: Float32 @0, ValueType: WeaponOverrideValueType @4 |
| WeaponPickupData | value type | - | 24 | LinkedToWeaponSlot: Int32 @0, MinAmmo: Uint32 @4, MaxAmmo: Uint32 @8, Weapon: SoldierWeaponAsset @12, WeaponSlot: Uint32 @16, AltWeaponSlot: Int32 @20 |
| WeaponSpecializationLink | value type | - | 8 | WeaponAsset: SoldierWeaponAsset @0, Specialization: PersistentSpecialization @4 |
| WeaponStateData | value type | - | 200 | UndeployTime: Float32 @0, AnimatedFireType: AnimatedFireEnum @4, DeployTime: Float32 @8, KeepAimingTime: Float32 @12, AltDeployTime: Float32 @16, AnimatedAimingType: AnimatedAimingEnum @20, CutsceneWeaponTransforms: StaticBone[] @24, ReadinessStates: ReadinessAnimations @40, Mesh3pTransforms: LinearTransform[] @44, HideProjectileAfterFireTime: Float32 @60, ProjectileBoneName: String @64, HiddenBones1p: String[] @76, BoneFakePhysics: BoneFakePhysicsData[] @92, HiddenBones3p: String[] @112, Meshes1p: SkinnedMeshAsset[] @128, MeshZoom1p: SkinnedMeshAsset @148, Meshes3p: SkinnedMeshAsset[] @152, ZoomedScopeFilter: ScopeFilterData @172, ZoomMeshTransitionFactor: Float32 @176, AnimTree3p: AnimTreeDataAsset @180, AnimTree1p: AnimTreeDataAsset @184, NonZoomedScopeFilter: ScopeFilterData @188, IsPumpAction: Boolean @192, SkipFireAnimation: Boolean @193, AlwaysAimHead: Boolean @194, IsOneHanded: Boolean @195, PlayDeployAfterFire: Boolean @196, SkipReloadAnimation: Boolean @197, SkipDeployAnimation: Boolean @198 |
| WeaponSwitchingMapData | value type | - | 24 | ToWeapon: WeaponSwitchingEnum[] @0, Action: EntryInputActionEnum @16, FromWeapon: WeaponSwitchingEnum @20 |
| ZoomLevelLockData | value type | - | 8 | LockType: LockType @0, OutlineTaggedDistance: Float32 @4 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: GameSharedResources value types 182]

## 2. Classes

### RTTI-GSR-3: The entity data classes

Status: **verified** · Variants: PC

The 188 `GameSharedResources` classes below `EntityData` (`Entity` module: `GameObjectData` -> `EntityData`, the
`Transform` at 48): what a level places. A placed one makes an entity only through a factory for its exact class
(SCR-EVT-3); which run-time class each makes is RTTI-FAC-1 (`gamedll.md`). Offsets are those the entities read
(for example `CapturePointEntityData.SquadSpawnRadius` at 180, `SpectateEntityData.Target` at 148 and
`UseInHardcoreMode` at 172, HUD-KILL-2).

<!-- typeinfo:gsr-entities -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AIMeshTestData | class | EntityData | 112 | - |
| AdvertisementComponentData | class | PartComponentData | 208 | AdTexture: String @176, Identifier: String @188 |
| AimAtOrderEntityData | class | OrderEntityData | 288 | Target: EntityData @272 |
| AlternateSpawnEntityData | class | EntityData | 128 | Priority: Float32 @112 |
| AmbientModelEntityData | class | GameEntityData | 224 | OffsetFromPath: Vec3 @128, BoundingBoxModifier: Vec3 @144, Speed: Float32 @160, InterpolationType: AmbientModelInterpolationType @164, Splines: AmbientModelWaypointSplineData[] @168, RotationType: AmbientModelRotationType @184, PathWaypoints: AreaGeometryEntityData @188, Effect: EffectAsset @192, MeshShaderSet: MeshShaderSetAsset @196, Mesh: RigidMeshAsset @200, TimeBetweenLods: Float32 @204, DisableOnLastWaypoint: Boolean @208 |
| AmmoBoxEntityData | class | ExplosionPackEntityData | 256 | AmmoResupply: AmmoResupplyData @240 |
| AmmoCrateEntityData | class | GameEntityData | 160 | Radius: Float32 @128, RefillDelay: Float32 @132, Model: StaticModelEntityData @136, Marker: MapMarkerEntityData @140, EnableReplenish: Boolean @144 |
| AnimationEntityData | class | GameEntityData | 144 | Soldier: SoldierSpawnBaseEntityData @128, Animation: AnimationAsset @132, AnimTree: AnimTreeData @136, LoopAnimation: Boolean @140 |
| AreaOrderEntityData | class | OrderEntityData | 304 | AnimationIndex: Int32 @272, IdleAnimation: AnimationAsset @276, Area: GeometryEntityData[] @280, AreaType: AreaOrderType @300 |
| AreaTriggerEntityData | class | TriggerEntityData | 208 | Include: AreaTriggerInclude @144, Radius: Float32 @148, InsideAreaEventRepeatTime: Float32 @152, Soldiers: SoldierSpawnBaseEntityData[] @156, Geometry: GeometryEntityData[] @176, ObjectToFollow: SpawnEntityData @196, TriggerOnControllable: Boolean @200, TriggerOnlyOnLeave: Boolean @201, OneInsideAreaEventPerSoldier: Boolean @202 |
| ArmDisarmAwardEntityData | class | GameEntityData | 128 | - |
| ArtilleryStrikeEntityData | class | GameEntityData | 224 | RadiusWhenFollowingSoldier: Float32 @128, HeightOffsetWhenFollowingSoldier: Float32 @132, MinDelayBetweenStrikes: Float32 @136, MinRadiusToSoldier: Float32 @140, MinProjectilesPerStrike: Uint32 @144, StrikeType: ArtilleryStrikeType @148, NumberOfStrikes: Uint32 @152, MaxProjectilesPerStrike: Uint32 @156, MaxDelayBetweenStrikes: Float32 @160, Squad: SquadBaseEntityData @164, Soldier: SoldierSpawnBaseEntityData @168, Geometry: GeometryEntityData[] @172, MissileData: MissileEntityData @192, MaxDelayBetweenProjectiles: Float32 @196, MinDelayBetweenProjectiles: Float32 @200, MissileSpawnRadius: Float32 @204, DurationOfArtilleryStrike: Float32 @208, FollowSoldier: Boolean @212 |
| AutoTeamEntityData | class | GameEntityData | 144 | TeamDifferenceToAutoBalance: Uint32 @128, PlayerCountNeededToAutoBalance: Uint32 @132, TeamAssignMode: TeamAssignMode @136, RotateTeamOnNewRound: Boolean @140, ForceIntoSquad: Boolean @141 |
| BangerEntityBaseData | class | DynamicPhysicsEntityData | 192 | MaxHealth: Float32 @160, TimeToLive: Float32 @164, Explosion: ExplosionEntityData @168, Mesh: MeshAsset @172, MeshShaderSet: MeshShaderSetAsset @176 |
| BangerEntityData | class | BangerEntityBaseData | 192 | - |
| BulletEntityData | class | MeshProjectileEntityData | 304 | DamageFalloffEndDistance: Float32 @208, TimeToArmExplosion: Float32 @212, DamageFalloffStartDistance: Float32 @216, StartDamage: Float32 @220, EndDamage: Float32 @224, AlternateTrailEffect: EffectAsset @228, AdditionalTrailEffectVelocityScale: Float32 @232, AdditionalTrailEffectMinDistanceToSpawn: Float32 @236, AdditionalTrailEffect: EffectAsset @240, TrailEffectVelocityScale: Float32 @244, TrailEffectMinDistanceToSpawn: Float32 @248, DetonationTimeVariation: Float32 @252, VehicleDetonationRadius: Float32 @256, VehicleDetonationActivationDelay: Float32 @260, FlyBySound: SoundAsset @264, Gravity: Float32 @268, ImpactImpulse: Float32 @272, DamageToDistribute: Float32 @276, Stamina: Float32 @280, DistributeDamageOverTime: Float32 @284, FlyBySoundRadius: Float32 @288, FlyBySoundSpeed: Float32 @292, InstantHit: Boolean @296, StopAdditionalTrailEffectOnUnspawn: Boolean @297, StopTrailEffectOnUnspawn: Boolean @298, HasVehicleDetonation: Boolean @299 |
| CSITextData | class | GameEntityData | 256 | ShadowOffset: Vec2 @128, Position: Vec2 @136, Color: Vec3 @144, ShadowColor: Vec3 @160, OutlineColor: Vec3 @176, TextRect: Vec4 @192, OutlineAlpha: Float32 @208, DelayBetweenChars: Float32 @212, ShadowAlpha: Float32 @216, Sid: String @220, FadeTime: Float32 @232, CursorDelayTime: Float32 @236, PauseBeforeFade: Float32 @240, ColorAlpha: Float32 @244, PauseDelay: Float32 @248 |
| CameraComponentData | class | ComponentData | 224 | CameraTransitionPos: Vec3 @128, MeshOffset: Vec3 @144, InputSuppression: InputSuppressionData @160, CameraTransitionTime: Float32 @176, MeshParentComponentNumber: Uint32 @180, Mesh: RigidMeshAsset @184, FreezeHeight: Float32 @188, FieldOfView: Float32 @192, AlternateView: AlternateCameraViewData @196, Camera: TargetCameraData @200, ReceiveImpulsesAsThirdPerson: Boolean @204, UseForegroundBlur: Boolean @205, IsSnapViewCamera: Boolean @206, IsFirstPerson: Boolean @207, UseAlternateViewSpecialization: Boolean @208, UseCameraTransition: Boolean @209 |
| CancelOrderEntityData | class | OrderEntityData | 272 | - |
| CapturePointEntityData | class | GameEntityData | 320 | ReturnMultiplier: Float32 @128, OnlyTakeableByTeam: Int32 @132, MinNrToTakeControl: Int32 @136, AreaValues: AreaValueTeam[] @140, TimeToGetControl: Int32 @156, TimeToLoseControl: Int32 @160, Index: Uint32 @164, IntruderWarningSID: String @168, SquadSpawnRadius: Float32 @180, MaxCaptureMultiplier: Int32 @184, AddedMultiplierPerPlayer: Float32 @188, EnemyTicketLossWhenCaptured: Int32 @192, MapMarker: MapMarkerEntityData @196, CapturePoint: StaticModelEntityData @200, LocationName: String @204, FlagTemplates: StaticModelEntityData[] @216, Spawners: SpawnEntityData[] @236, Geometry: GeometryEntityData[] @256, AreaValueTeam1: Int32 @276, AreaValueTeam2: Int32 @280, CaptureRadius: Float32 @284, InitialOwnerTeam: TeamId @288, ControlPointName: String @292, ShowOnMinimap: Boolean @304, HoistFlag: Boolean @305, StartAtBottom: Boolean @306, LoseControlWhenNotClose: Boolean @307, SeesawCapturing: Boolean @308, RemoveWhenCaptured: Boolean @309, DisableIfEnemyInside: Boolean @310, IntruderWarning: Boolean @311, IsCapturedInUpperSphere: Boolean @312, IsVisible: Boolean @313, UnableToChangeTeam: Boolean @314, DisableWhenLosingControl: Boolean @315 |
| ChangeFormationOrderEntityData | class | OrderEntityData | 288 | Scale: Float32 @272, Formation: FormationData @276 |
| ChangeOrderEntityData | class | OrderEntityData | 304 | DisallowedIndices: Int32[] @272, Vehicle: VehicleSpawnEntityData @288, Instant: Boolean @292 |
| ChangeReadinessOrderEntityData | class | OrderEntityData | 288 | Minimum: ReadinessLevel @272, Pulse: ReadinessLevel @276, AllowDecrease: Boolean @280 |
| CharacterStateTriggerEntityData | class | TriggerEntityData | 240 | HealthToSet: Uint32 @144, HealthToCheck: Uint32 @148, EntryInputAction: EntryInputActionEnum @152, VehiclePosition: Uint32 @156, ForcePlayerEnterEntry: Uint32 @160, AmmoInClipToSet: Uint32 @164, PoseToCheck: CharacterPoseType @168, AmmoInPoolToSet: Uint32 @172, Damage: Float32 @176, DamageInterval: Float32 @180, Pitch: Float32 @184, Vehicles: VehicleSpawnEntityData[] @188, NewVehiclePositionOnCharacterMove: AlternateSpawnEntityData @208, PlayerToCheck: SoldierSpawnBaseEntityData @212, NewPosition: AlternateSpawnEntityData @216, CharacterPose: CharacterPoseType @220, Weapon: SoldierWeaponAsset @224, CheckVehiclePosition: Boolean @228, CheckAllWeapons: Boolean @229, ContinueToCheckIfHoldingWrongWeapon: Boolean @230, ExcludeConnectedVehicles: Boolean @231, SuppressInputOnInputTriggerd: Boolean @232, UsePitch: Boolean @233, CheckHealthOnAndAbove: Boolean @234 |
| ChildComponentData | class | PartComponentData | 256 | SoundEffectStartRpm: Float32 @176, SoundEffectStopRpm: Float32 @180, HealthZone: VehicleHealthZoneData @192, SoundEffect: SoundAsset @240, MovingBody: MovingBodyData @244 |
| ClearAreaTriggerEntityData | class | TriggerEntityData | 208 | TeamOfImmortalSoldiers: TeamId @144, SoldiersToExclude: SoldierSpawnBaseEntityData[] @148, Action: ClearType @168, Areas: GeometryEntityData[] @172, Area: GeometryEntityData @192, RemoveBangers: Boolean @196, ExcludeImmortalSoldiersInTeam: Boolean @197, RemoveStaticModels: Boolean @198, RemoveSoldiers: Boolean @199, RemoveVehicles: Boolean @200, RemoveEffects: Boolean @201 |
| CombatActionTriggerEntityData | class | TriggerEntityData | 160 | TimeToTriggerOnNoAction: Float32 @144, Geometry: GeometryEntityData @148 |
| CombatAreaTriggerEntityData | class | GameEntityData | 304 | StrikeRadius: Float32 @128, MaxRandomSpawnHeight: Float32 @132, InitalSpeed: Float32 @136, Camera: CombatAreaCamera @144, Team: TeamId @224, SpawnHeight: Float32 @228, Geometry: GeometryEntityData[] @232, SurroundingGeometry: GeometryEntityData[] @252, TimeToReturn: Float32 @272, AllowedVehicles: VehicleAsset[] @276, DeserterProjectile: BulletEntityData @296, IsAiAllowedOutside: Boolean @300, UseTeamRestriction: Boolean @301 |
| ConsoleCommandEntityData | class | TestEntityData | 144 | Commands: String[] @128 |
| ConsumableTriggerEntityData | class | GameEntityData | 144 | Persistence: Boolean @128 |
| ContinueEntityData | class | GameEntityData | 176 | FadeToBlack: Float32 @128, StayBlack: Float32 @132, FadeFromBlack: Float32 @136, WeaponAmmo: ContinueWeaponAmmoData[] @140, VehicleImmortalTimer: Float32 @156, SoldierImmortalTimer: Float32 @160, LookAtNearestMapMarker: Boolean @164 |
| ControllableEntityData | class | GamePhysicsEntityData | 192 | LockedWarnSoundEffect: SoundAsset @144, LastKnownSpottedTime: Float32 @148, PassiveSpottedTime: Float32 @152, ActiveSpottedTime: Float32 @156, LowHealthWarnSoundEffect: SoundAsset @160, LockingWarnSoundEffect: SoundAsset @164, LowHealthTreshold: Float32 @168, MissileWarnSoundEffect: SoundAsset @172, AimWarnSoundEffect: SoundAsset @176, UsePrediction: Boolean @180, IsSpottedWhenDamaged: Boolean @181 |
| CoverEntityData | class | EntityData | 128 | Material: CoverType @112, RightBlocked: Boolean @116, LeftBlocked: Boolean @117 |
| CoverModifierData | class | EntityData | 128 | Manipulation: CoverManipulationType @112 |
| CutsceneCameraEntityData | class | CutsceneObjectEntityBaseData | 240 | Camera: CutsceneEntityCameraData @224, ActiveFromStart: Boolean @228 |
| CutsceneEntityData | class | GameEntityData | 208 | SoundLength: Float32 @128, Radius: Float32 @132, Sequence: SequenceData @136, AfterCutsceneMenuId: MenuIdEnum @140, CutsceneMenuId: MenuIdEnum @144, Subtitle: SubtitleDataTree @148, ScreenFade: CutsceneEntityScreenFadeData @152, Camera: CutsceneEntityCameraData @156, CameraEntitySpace: LogicEntityData @160, SoundTemplate: String @164, LocalizedSound: WaveAsset[] @176, Sound: WaveAsset @196, FreezeOnFinish: Boolean @200, AllowAllAnimationsHeaps: Boolean @201, MuteVoiceOver: Boolean @202, NormalizedTimeEnabled: Boolean @203, MuteGameSound: Boolean @204, IsSkippable: Boolean @205, KeepGameSoundMuted: Boolean @206, ForceMuteSoundSetting: Boolean @207 |
| CutsceneObjectEntityBaseData | class | GameEntityData | 224 | InitialRootBoneTransform: LinearTransform @128, ScaleAnimationPlaybackTime: Float32 @192, ScaleAnimationPlaybackTimeFade: Float32 @196, AnimTree: AnimTreeData @200, EntitySpace: LogicEntityData @204, TotalLength: Float32 @208, UseNormalizedTime: Boolean @212, IsLooping: Boolean @213 |
| CutsceneObjectEntityData | class | CutsceneObjectEntityBaseData | 320 | BoundingBoxModifier: Vec3 @224, AdditionalMeshes: MeshAsset[] @240, TrackBone: String @260, SnapToTerrainOffset: Float32 @272, Mesh: MeshAsset @276, AnimationSequence: CutsceneObjectAnimationData[] @280, Animation: AnimationAsset @296, OriginalAnimationIndex: Int32 @300, AnimationIndex: Int32 @304, KeepMeshAfterAnimation: Boolean @308, SnapToTerrain: Boolean @309 |
| CutsceneSoldierEntityData | class | CutsceneObjectEntityBaseData | 288 | EyesAndYawAnimationLength: Float32 @224, CutsceneBlendInTime: Float32 @228, CancelInterludeOrder: CancelOrderEntityData @232, BlendInFromLocalSoldierTime: Float32 @236, CharacterAnimationLength: Float32 @240, FacialAnimationLength: Float32 @244, InterludeOrder: InterludeOrderEntityData @248, EyesAndYawAnimation: AnimationAsset @252, FacialAnimation: AnimationAsset @256, CharacterAnimation: AnimationAsset @260, Weapon: SoldierWeaponAsset @264, SoldierPositionAfterCutscene: StaticPositionEntityData @268, SpawnPoint: SoldierSpawnBaseEntityData @272, SnapSoldierToTerrain: Boolean @276, StaticEntitySpace: Boolean @277, SoldierImmortalDuringCutscene: Boolean @278, LeaveVehicleEntry: Boolean @279, AnimationCullingEnabled: Boolean @280, AnimateWeapon: Boolean @281, CollisionEnabled: Boolean @282, DisableAI: Boolean @283 |
| CutsceneVehicleEntityData | class | CutsceneObjectEntityBaseData | 272 | AnimationFarPlaybackspeedDistance: Float32 @224, AnimationNearPlaybackspeedDistance: Float32 @228, RootBone: String @232, SpawnPoint: VehicleSpawnEntityData @244, Animation: AnimationAsset @248, AnimationFadeInTime: Float32 @252, AnimationScalePlaybackSpeed: Float32 @256, PlayOnClient: Boolean @260 |
| DamageAreaTriggerEntityData | class | TriggerEntityData | 160 | DamageTime: Float32 @144, DamagePerSecond: Float32 @148, Area: GeometryEntityData @152, DamageVehicles: Boolean @156, DamageSoldiers: Boolean @157, DamageBangers: Boolean @158 |
| DeathAreaTriggerEntityData | class | GameEntityData | 160 | TimeToReturn: Float32 @128, Damage: Float32 @132, Geometry: GeometryEntityData[] @136 |
| DebugMenuItemEntityData | class | TestEntityData | 160 | Text: String @128, ItemType: DebugMenuItemType @140, Parent: DebugMenuSubMenuEntityData @144 |
| DebugMenuSubMenuEntityData | class | TestEntityData | 144 | Text: String @128 |
| DebugTestEntityData | class | GameEntityData | 128 | - |
| DebugTextEntityData | class | TestEntityData | 160 | TextColor: Vec3 @128, DebugText: String @144, Centered: Boolean @156 |
| DelayTriggerEntityData | class | TriggerEventEntityData | 160 | AutoTriggerEvent: Boolean @144 |
| DetailedTreeBangerEntityData | class | TreeBangerEntityData | 240 | - |
| DisableTeamEntityData | class | GameEntityData | 144 | Geometry: GeometryEntityData @128, Team: TeamId @132, DisableType: DisableTeamType @136 |
| DynamicModelEntityData | class | DynamicPhysicsEntityData | 176 | MeshShaderSet: MeshShaderSetAsset @160, Mesh: MeshAsset @164, Part: PartComponentData @168 |
| DynamicPhysicsEntityData | class | GamePhysicsEntityData | 160 | TrackedByAI: Boolean @144 |
| DynamicWeaponPickupEntityData | class | PickupEntityData | 192 | WeaponSlots: DynamicWeaponPickupSlotData[] @160, KeepAmmoState: Boolean @176 |
| EffectComponentData | class | ComponentData | 144 | StopTrigger: EffectComponentTrigger @128, StartTrigger: EffectComponentTrigger @132, Effect: EffectAsset @136 |
| EndOfRoundSoldierSpawnEntityData | class | GameEntityData | 144 | Team: TeamId @128, Rank: Uint32 @132 |
| EngineComponentData | class | ComponentData | 160 | SurfaceSoundEffect: SoundAsset @128, AudioMaterials: AudioMaterialDefinitions @132, SimpleEngineSoundEffect: SoundAsset @136, Config: EngineConfigData @140, SoundEffect: SoundAsset @144 |
| EntryComponentData | class | ComponentData | 240 | SoldierOffset: Vec3 @128, AnimationOffset: Vec3 @144, EntryRadius: Float32 @160, EnterImpulse: Float32 @164, PoseConstraints: PoseConstraintsData @168, AnimationParametersFromEntry: Int32 @172, EntryOrderNumber: Int32 @176, InputConceptDefinition: EntryInputActionMapsData @180, EntryClass: EntryClass @184, AIData: ObjectData @188, InputMapping: InputActionMappingsData @192, VOTypes: VOVehicle[] @196, HudData: EntryComponentHudData @216, HasDeathAnimation: Boolean @224, RagdollCollideWithVehicles: Boolean @225, IsShielded: Boolean @226, LockSoldierAimingToEntry: Boolean @227, IsAllowedToExitInAir: Boolean @228, ShowSoldierInEntry: Boolean @229, ShowSoldierGearInEntry: Boolean @230, ShowSoldierWeaponInEntry: Boolean @231 |
| EventGateEntityData | class | GameEntityData | 144 | Timer: Float32 @128, PassThrough: Boolean @132 |
| EventIfSwitchEntityData | class | GameEntityData | 144 | RunOnce: Boolean @128, StartState: Boolean @129 |
| EventMemoryEntityData | class | GameEntityData | 128 | - |
| EventSequenceTriggerEntityData | class | TriggerEventEntityData | 160 | Random: Boolean @144 |
| EventSplitterEntityData | class | LogicUtilEntityData | 144 | OnImpulseDelayTime: Float32 @128, RunOnce: Boolean @132 |
| ExplosionEntityData | class | GameEntityData | 208 | ShockwaveDamage: Float32 @128, BlastImpulse: Float32 @132, BlastRadius: Float32 @136, ShockwaveTime: Float32 @140, ShockwaveImpulse: Float32 @144, ShockwaveRadius: Float32 @148, InnerBlastRadius: Float32 @152, Material: String @156, SootDecal: String @168, DetonationEffect: EffectAsset @180, BlastDamage: Float32 @184, CoreRadius: Float32 @188, EmpTime: Float32 @192, UseAsPreDestuctionEntity: Boolean @196, UnspotsOnExplode: Boolean @197, DisableOcclusion: Boolean @198, DisableStaticEntityOcclusion: Boolean @199 |
| ExplosionImpulseTestEntityData | class | TestEntityData | 144 | Explosion: ExplosionEntityData @128, explosionStrength: Float32 @132, fireRate: Float32 @136 |
| ExplosionPackEntityData | class | MeshProjectileEntityData | 240 | Health: Float32 @208, TimeToLiveOnPlayerDeath: Float32 @212, ImpulseScale: Float32 @216, ActiveSpottedTime: Float32 @220, AttachSound: SoundAsset @224, VehicleDetonationRadius: Float32 @228, VehicleDetonationActivationDelay: Float32 @232, HasVehicleDetonation: Boolean @236, HasRemoteDetonator: Boolean @237, IsDestructible: Boolean @238, ReceivesExplosionDamage: Boolean @239 |
| FenceModelEntityData | class | StaticModelEntityData | 368 | Angle: Float32 @352, Width: Float32 @356 |
| FireAtAreaOrderEntityData | class | OrderEntityData | 304 | Area: GeometryEntityData[] @272, StopIfPlayerIsSensed: Boolean @292 |
| FollowAISoldierOrderEntityData | class | OrderEntityData | 304 | Target: SoldierSpawnBaseEntityData[] @272 |
| FollowClosestHumanOrderEntityData | class | OrderEntityData | 272 | - |
| ForbiddenAIAreaData | class | GameEntityData | 144 | Geometry: GeometryEntityData @128, StartForbidden: Boolean @132 |
| ForceAreaEntityData | class | GameEntityData | 176 | BangerForceMultiplier: Float32 @128, VehicleForceMultiplier: Float32 @132, ParticleForceMultiplier: Float32 @136, RagdollForceMultiplier: Float32 @140, ForceType: ForceType @144, Areas: GeometryEntityData[] @148, SoldierForceMultiplier: Float32 @168, Force: Float32 @172 |
| ForestEntityData | class | GroupEntityData | 176 | - |
| GameEffectEntityData | class | GameEntityData | 208 | Offset: Vec3 @128, Rotation: Vec3 @144, CameraProximityDistanceScale: Float32 @160, ProximityTriggerPriority: EffectPriority @164, Multiplier: Float32 @168, Effect: EffectAsset @172, EntitySpace: LogicEntityData @176, BoneName: String @180, PlayFromClient: Boolean @192, IsCameraProximityTriggered: Boolean @193 |
| GameEntityData | class | LogicEntityData | 128 | - |
| GameInteractionEntityData | class | InteractionEntityData | 192 | TeamId: TeamId @160, HoldToInteractTime: Float32 @164, StaticModel: ReferenceObjectData @168, InteractionEntityType: InteractionEntityType @172, OnInteractionSendDelay: Float32 @176, DelayBetweenUses: Float32 @180, IsHumanAllowed: Boolean @184, SendOnInteractionOnAnimationStart: Boolean @185 |
| GamePhysicsEntityData | class | GameEntityData | 144 | Material: String @128, PhysicsData: PhysicsEntityData @140 |
| GameSettingEntityData | class | GameEntityData | 144 | ChangeSetting: ChangeGameSettingType @128 |
| GameSoundEntityData | class | GameEntityData | 144 | Sound: SoundAsset @128, SaveState: Boolean @132, PlayFromClient: Boolean @133, StartForTriggeringPlayer: Boolean @134 |
| GameTestEntityData | class | TestEntityData | 144 | TimeoutTime: Float32 @128 |
| GarrisonEntityData | class | GameEntityData | 144 | CreateSoldierOnSpawn: Boolean @128 |
| GrenadeEntityData | class | MeshProjectileEntityData | 240 | GrenadeProximityRadius: Float32 @208, GrenadeProximityTime: Float32 @212, MinBounceSpeed: Float32 @216, Gravity: Float32 @220, CollisionSpeedMultiplier: Float32 @224 |
| GroupEntityData | class | GamePhysicsEntityData | 176 | MemberDatas: GroupMemberData[] @144 |
| HudCounterEntityData | class | GameEntityData | 160 | Position: Vec2 @128, DecreaseStep: Uint32 @136, CounterType: HudCounterType @140, IncreaseStep: Uint32 @144, StartValue: Uint32 @148, MaxValue: Uint32 @152, MinValue: Uint32 @156 |
| HudEntityData | class | GameEntityData | 128 | - |
| HumanPlayerEntityData | class | GameEntityData | 160 | DisplayTime: Float32 @128, LastManStandingSid: String @132, PlayerKilledDelay: Float32 @144 |
| InputRestrictionEntityData | class | GameEntityData | 160 | IngameMenu: Boolean @128, Sprint: Boolean @129, SwitchPrimaryWeapon: Boolean @130, ScoreboardMenu: Boolean @131, ToggleParachute: Boolean @132, ChangePose: Boolean @133, ToggleCamera: Boolean @134, Reload: Boolean @135, MeleeAttack: Boolean @136, DynamicGadget2: Boolean @137, Order: Boolean @138, ThrowGrenade: Boolean @139, StaticGadget: Boolean @140, GrenadeLauncher: Boolean @141, DynamicGadget1: Boolean @142, SnapView: Boolean @143, Yaw: Boolean @144, HandBrake: Boolean @145, Roll: Boolean @146, Pitch: Boolean @147, Throttle: Boolean @148, OverridePreviousInputRestriction: Boolean @149, Brake: Boolean @150, Strafe: Boolean @151, Jump: Boolean @152, Zoom: Boolean @153, ChangeEntry: Boolean @154, ChangeVehicle: Boolean @155, FireCountermeasure: Boolean @156, Fire: Boolean @157, CycleRadioChannel: Boolean @158, AltFire: Boolean @159 |
| InteractableStaticModelEntityData | class | StaticModelEntityData | 352 | - |
| InteractionEntityData | class | GameEntityData | 160 | MaxUses: Uint32 @128, PreInteractionSoundEffect: SoundAsset @132, UseWithinAngle: Float32 @136, SoldierAnimTree: AnimTreeDataAsset @140, UseWithinRadius: Float32 @144, TestIfOccluded: Boolean @148 |
| InterludeOrderEntityData | class | OrderEntityData | 288 | Crouch: Boolean @272, IsCutscene: Boolean @273 |
| JoinSquadOrderEntityData | class | OrderEntityData | 288 | SquadToJoin: SquadBaseEntityData @272 |
| KillAllEntityData | class | GameEntityData | 144 | KillAllHumanSoldiers: Boolean @128 |
| KillCounterEntityData | class | GameEntityData | 144 | MaxKillCount: Int32 @128, TeamId: TeamId @132 |
| KitPickupEntityData | class | PickupEntityData | 176 | Kit: SoldierAsset @160, KeepAmmoState: Boolean @164, KeepAdditionalWeapons: Boolean @165 |
| LadderEntityData | class | StaticModelEntityData | 352 | - |
| LifeCounterEntityData | class | GameEntityData | 224 | ReinforceTable: Float32[] @128, FallbackTime: Float32 @144, BaseCount: Int32 @148, ReinforceThreshold: Int32 @152, ReinforceMessageSid: String @156, DisplayTime: Float32 @168, FightHarderMessageSid: String @172, FightHarderTreshold: Float32 @184, UiShowCountUpperThreshold: Int32 @188, UiShowCountLowerThreshold: Int32 @192, LifeTresholdValues: Int32[] @196, TeamId: TeamId @212, StartingLifeCount: Int32 @216, Attacker: Boolean @220, SimpleReinforce: Boolean @221, SetBestSquadSpawner: Boolean @222 |
| LockingControllerData | class | ComponentData | 176 | HoldStillThreshold: Float32 @128, SampleRate: Float32 @132, MinimumLockTime: Float32 @136, RayLength: Float32 @140, LockTime: Float32 @144, ZoomLevelLock: ZoomLevelLockData[] @148, ReleaseOnNewTargetTime: Float32 @164, ReleaseTime: Float32 @168, LockOnVisibleTargetsOnly: Boolean @172, LockOnWorldSpacePos: Boolean @173, PositionOnly: Boolean @174 |
| LogicUtilEntityData | class | GameEntityData | 128 | - |
| LookAtTriggerEntityData | class | TriggerEventEntityData | 176 | ZoomLevel: Uint32 @144, MinDistanceToObject: Float32 @148, MaxDistanceToObject: Float32 @152, ObjectToFollow: SoldierSpawnBaseEntityData @156, FOV: Float32 @160, Weapon: SoldierWeaponAsset @164, BinocularsMode: Boolean @168 |
| MapMarkerEntityData | class | GameEntityData | 192 | ColorType: MapMarkerColorType @128, MarkerType: MapMarkerType @132, BlinkTime: Float32 @136, FocusPointRadius: Float32 @140, HudIcon: UIHudIcon @144, VisibleForTeam: TeamId @148, MultipleTrigger: MultipleTriggerEntityData @152, StaticModel: ReferenceObjectData @156, Controllable: SpawnEntityData @160, Radius: Float32 @164, Sid: String @168, ShowAirTargetBox: Boolean @180, IsFocusPoint: Boolean @181, IsVisible: Boolean @182 |
| MedicBagEntityData | class | ExplosionPackEntityData | 256 | Healer: HealerData @240 |
| MenuEntityData | class | GameEntityData | 160 | BodySid: String @128, HeaderSid: String @140, MenuId: MenuIdEnum @152 |
| MeshProjectileEntityData | class | ProjectileEntityData | 208 | InitialAngularVelocity: Vec3 @176, Mesh: MeshAsset @192, TrailEffect: EffectAsset @196, ExtraDamping: Boolean @200, IsAttachableToSoldiers: Boolean @201, IsAttachable: Boolean @202, IsAttachableToVehicles: Boolean @203 |
| MessageEntityData | class | GameEntityData | 160 | SquadId: SquadId @128, DisplayTime: Float32 @132, TeamId: TeamId @136, MessageSid: String @140, Reciever: MessageReciever @152 |
| MiddlegrowthModelEntityData | class | GamePhysicsEntityData | 176 | DestructionEffects: EffectAsset[] @144, SoundCollisionScale: Float32 @164, Mesh: TreeMeshAsset @168 |
| MiddlegrowthModelGroupEntityData | class | GroupEntityData | 176 | - |
| MissileEntityData | class | MeshProjectileEntityData | 288 | Gravity: Float32 @208, FlyBySoundRadius: Float32 @212, MaxTurnAngle: Float32 @216, Drag: Float32 @220, Damage: Float32 @224, WarnTarget: WarnTarget @228, FlyBySoundSpeed: Float32 @232, ImpactImpulse: Float32 @236, EngineStrength: Float32 @240, MaxSpeed: Float32 @244, EngineEffect: EffectAsset @248, FlyBySound: SoundAsset @252, EngineTimeToIgnition: Float32 @256, TimeToActivateGuidingSystem: Float32 @260, TimeToArm: Float32 @264, EngineTimeToLive: Float32 @268, ApplyGravityWhenUnguided: Boolean @272 |
| MissionEntityData | class | GameEntityData | 192 | FailureSid: String @128, NextLevelToRun: String @140, BriefingSid: String @152, SuccessSid: String @164, RollCreditsOnSuccess: Boolean @176, TransferWeaponsToNextLevel: Boolean @177 |
| MissionObjectiveEntityData | class | LogicUtilEntityData | 256 | ObjectiveSid: String @128, ObjectiveBriefSid: String @140, OnSuccessDelayTime: Float32 @152, OnShowObjectiveDelayTime: Float32 @156, TeamId: TeamId @160, SquadId: SquadId @164, SuccessSid: String @168, Reciever: MessageReciever @180, ParentObjective: MissionObjectiveEntityData @184, HudData: MissionObjectiveHudData @188, MapMarkers: MapMarkerEntityData[] @192, ChildObjective: MissionObjectiveEntityData[] @212, SuccessDelayTime: Float32 @232, ShowObjectiveDelayTime: Float32 @236, ObjectiveType: ObjectiveType @240, DisplayTime: Float32 @244, DisplaySuccess: Boolean @248 |
| MotionSensorEntityData | class | GrenadeEntityData | 288 | TimeToShowEnemies: Float32 @240, SoldierVehicleMinSpeed: Float32 @244, TeamToEffect: TeamId @248, PulseDelay: Float32 @252, SpottingSound: SoundAsset @256, Radius: Float32 @260, TimeToActivate: Float32 @264, ShowExplosives: Boolean @268, DisplayInMiniMap: Boolean @269, OnlyShowEnemies: Boolean @270, ShowSoldiers: Boolean @271, ShowVehicles: Boolean @272 |
| MultipleTriggerEntityData | class | TriggerEventEntityData | 176 | OutputEventAmount: Uint32 @144, TriggerAmount: Uint32 @148, EntityTriggerAmount: GameEntityData[] @152 |
| OrderEntityData | class | GameEntityData | 272 | ReadinessOnComplete: ReadinessLevel @128, EventsOnFail: OrderEventSettings @132, AllowStunAnimations: StunAllowState @148, EventsOnComplete: OrderEventSettings @152, SpeedState: SpeedStateOverride @168, AimState: AimStateOverride @172, ReadinessOnFail: ReadinessLevel @176, Stance: StanceOverride @180, ReadinessOnStart: ReadinessLevel @184, IssueOrderDelay: Float32 @188, OnCompleteTimeOut: Float32 @192, Squads: SquadBaseEntityData[] @196, Soldiers: SoldierSpawnBaseEntityData[] @216, OnFailedTimeOut: Float32 @236, EventsOnStart: OrderEventSettings @240, ReissueIfSoldierIsKilled: Boolean @256, UnspawnOnComplete: Boolean @257, UnspawnOnFail: Boolean @258, IsUrgent: Boolean @259 |
| PartComponentData | class | ComponentData | 176 | NeighbourLinks: Uint32[] @128, HealthStates: HealthStateData[] @144, CanTriggerCollapse: Boolean @164, CanCollapse: Boolean @165, IsLoadBearing: Boolean @166, IsSupported: Boolean @167, IsFragile: Boolean @168 |
| PatrolOrderEntityData | class | OrderEntityData | 320 | DefaultSpeed: Float32 @272, VehicleThrottle: Float32 @276, MinDistance: Float32 @280, RouteWaypoints: AreaGeometryEntityData[] @284, TypeOfRoute: RouteType @304, UsePathFinding: Boolean @308 |
| PickupEntityData | class | GameEntityData | 160 | InteractionRadius: Float32 @128, TimeToLive: Float32 @132, Mesh: MeshAsset @136, MeshShaderSet: MeshShaderSetAsset @140, ForceWeaponSlotSelection: Boolean @144, IgnoreNullWeaponSlots: Boolean @145, DisplayInMiniMap: Boolean @146, UseHudSelection: Boolean @147, HasAutomaticAmmoPickup: Boolean @148, UnspawnOnPickup: Boolean @149, UseWeaponMesh: Boolean @150, UseTeamShaderSet: Boolean @151, PositionIsStatic: Boolean @152, ContentIsStatic: Boolean @153, UnspawnOnAmmoPickup: Boolean @154 |
| PlayVideoEntityData | class | GameEntityData | 144 | BinkMovie: BinkTextureAsset @128, KeepBlackScreen: Boolean @132 |
| PlayerEntryComponentData | class | EntryComponentData | 256 | AnimTree: AnimTreeDataAsset @240 |
| PreRoundEntityData | class | GameEntityData | 144 | NotEnoughPlayersRestartTimer: Float32 @128, PlayerCountNeeded: Uint32 @132 |
| PredestructionEntityData | class | GameEntityData | 144 | Radius: Float32 @128 |
| ProjectileEntityData | class | GamePhysicsEntityData | 176 | InitMeshHideTime: Float32 @144, VisualConvergeDistance: Float32 @148, Explosion: ExplosionEntityData @152, InitialSpeed: Float32 @156, TimeToLive: Float32 @160, MaxCount: Uint32 @164, DetonateOnTimeout: Boolean @168 |
| ProjectileImpulseTestEntityData | class | TestEntityData | 144 | Projectile: ProjectileEntityData @128, fireRate: Float32 @132 |
| RadarSweepComponentData | class | ComponentData | 160 | PulseDelay: Float32 @128, TimeToShowEnemies: Float32 @132, SoldierVehicleMinSpeed: Float32 @136, SpottingSound: SoundAsset @140, Radius: Float32 @144, TimeToActivate: Float32 @148, ShowVehicles: Boolean @152, ShowExplosives: Boolean @153, OnlyShowEnemies: Boolean @154, ShowSoldiers: Boolean @155 |
| RadioComponentData | class | ComponentData | 144 | SoundEffect: SoundAsset @128 |
| RandomTriggerEntityData | class | TriggerEventEntityData | 160 | RandomEventWeight: Float32[] @144 |
| RandomWalkOrderEntityData | class | OrderEntityData | 304 | Geometry: GeometryEntityData[] @272 |
| RemoteEntryComponentData | class | EntryComponentData | 256 | Dummy: Int32 @240 |
| RigidModelEntityData | class | GamePhysicsEntityData | 160 | MeshShaderSet: MeshShaderSetAsset @144, Mesh: RigidMeshAsset @148 |
| RigidModelGroupEntityData | class | GroupEntityData | 176 | - |
| RotorComponentData | class | ComponentData | 192 | BlowEffect: EffectAsset @128, ChangeModelRpm: Float32 @132, CriticallyDamagedRotationForce: Float32 @136, TriggerGroundEffectHeight: Float32 @140, LowRpmModel: RotorModelData @144, HighRpmModel: RotorModelData @160, RotationMultiplier: Float32 @176, RotationAxis: RotationAxis @180, OnlyTriggerBlowEffectInWater: Boolean @184 |
| SaveEntityData | class | LogicUtilEntityData | 176 | SaveNameSID: String @128, SaveDelayTime: Float32 @140, SaveFileName: String @144, SaveScreenTexture: TextureAsset @156, LevelName: String @160 |
| ScreenEffectEntityData | class | GameEntityData | 224 | EndState: FrameEffectState @128, FadeOutTime: Float32 @160, InitialState: FrameEffectState @176, FadeInTime: Float32 @208, FrameEffectTexture: TextureAsset @212, BlendMode: FrameEffectBlendMode @216, TriggerFrameEffect: Boolean @220 |
| ScreenFadeEntityData | class | GameEntityData | 144 | FadeTime: Float32 @128, FadeUI: Boolean @132, FadeInGameSound: Boolean @133, ControlledByClient: Boolean @134, WaitOnBlackForStreaming: Boolean @135 |
| SearchAndDestroyOrderEntityData | class | OrderEntityData | 336 | TargetPosKnowledge: TargetPosKnowledgeType @272, FindTargetTime: Float32 @276, TargetBotSoldier: SoldierSpawnBaseEntityData[] @280, Area: GeometryEntityData[] @300, Aggressive: Boolean @320, TargetAreaFollowsTarget: Boolean @321 |
| ServerConnectTriggerEntityData | class | GameEntityData | 128 | - |
| ShootAtObjectOrderEntityData | class | OrderEntityData | 288 | TargetGroup: TargetObjectDataGroup @272, IgnoreSensing: Boolean @276 |
| SkinnedMeshComponentData | class | ComponentData | 160 | Skeleton: SkeletonAsset @128, Mesh: SkinnedMeshAsset @132, BoneFakePhysics: BoneFakePhysicsData[] @136 |
| SoldierEntityData | class | ControllableEntityData | 560 | CriticalHealthJesusModeTime: Float32 @192, ImmortalTimeAfterSpawn: Float32 @196, RegenerateHealth: Float32 @200, WaterDamage: Float32 @204, CameraStrafeTiltStrength: Float32 @208, ReviveTime: Float32 @212, SoldierOnFireHitsWater: EffectAsset @216, FireBulletHitEffect: EffectAsset @220, DeathBulletHitEffect: EffectAsset @224, RegenGracePeriod: Float32 @228, MaxHealth: Float32 @232, ImpulseReactionTime: Float32 @236, Specializations: PersistentSpecialization[] @240, ValidRoadkillTime: Float32 @260, NeedMoreAmmoPercentage: Float32 @264, Licenses: String @268, MeshShaderSetNumberOverride: Int32 @280, Bucket: ScoringBucket @284, Category: KitCategory @288, TimeForCorpse: Float32 @292, BelowWaterDamageDelay: Float32 @296, NeedMoreHealthPercentage: Float32 @300, CombatReadinessCooldown: Float32 @304, VOTypes: VOPerson[] @308, Cameras: TargetCameraData[] @328, FakeCollisionData: FakeAnimatedSoldierCollisionData @348, BoneFakePhysics: BoneFakePhysicsData[] @352, HeadCollision: SoldierHeadCollisionData @372, AimingConstraints: SoldierAimingConstraintsData @376, AutoAim: SoldierAutoAimData @380, WeaponSwitching: WeaponSwitchingData @384, HeadMaterial: String @388, FootMaterial: String @400, WeaponDataOverride: SoldierWeaponOverrideData @412, WeaponUnlocks: SoldierWeaponUnlockData[] @416, Weapons: SoldierWeaponAsset[] @432, Meshes1p: SoldierMeshData[] @452, Meshes3p: SoldierMeshData[] @468, StreamGroup1p: StreamGroupData @484, AnimTree3p: SoldierAnimationTreeData @488, DeathPickup: PickupEntityData @492, Sound: SoldierSoundData @496, HidableMeshParts: HidableSoldierMeshWeaponPart[] @500, ParachuteData: SoldierParachuteData @516, CharacterPhysics: CharacterPhysicsData @520, SprintSettings: SoldierSprintSettingsData @524, RagdollSettings: SoldierRagdollSettingsData @528, AnimTree1p: SoldierAnimationTreeData @532, ShareClientAnimationWithServer: Boolean @536, LowerGunOnOwnTeam: Boolean @537, LockAimToMeleeAttacker: Boolean @538, ProximityCheck: Boolean @539, EnableGroundmapLighting: Boolean @540 |
| SoldierEntryComponentData | class | EntryComponentData | 272 | SoldierEntryReadinessRestriction: ReadinessRestriction @240, AnimationPoseRestriction: PoseRestriction @244, AimingConstraints: AimingConstraintsData @248, AnimTree: AnimTreeDataAsset @264, SoldierLowerBodyFixed: Boolean @268 |
| SoldierGripComponentData | class | ComponentData | 128 | - |
| SoldierSpawnBaseEntityData | class | SpawnEntityData | 160 | - |
| SoldierSpawnEntityData | class | SoldierSpawnBaseEntityData | 288 | LodDistance: Float32 @160, HumanTargetPreference: Float32 @164, VehiclePos: Uint32 @168, LocationNameSid: String @172, SpawnEnemyRadius: Float32 @184, SoldierSquadId: SquadId @188, OnSpottedType: SoldierOnSpottedType @192, SpawnToCheck: Uint32 @196, Vehicle: VehicleSpawnEntityData[] @200, AlternativeSpawnPoints: AlternateSpawnEntityData[] @220, Squad: SquadEntityData[] @240, SoldierKit: SoldierAsset @260, PlayerType: SoldierSpawnInclude @264, MeshShaderSetNumber: Uint32 @268, Template: SoldierSpawnTemplateData @272, TryToSpawnAwayFromSight: Boolean @276, SoldierCreationPoint: Boolean @277, SpawnSoldierFromBeginning: Boolean @278, UseAsAlternateSpawnPoint: Boolean @279 |
| SoldierSpawnGhostEntityData | class | SoldierSpawnBaseEntityData | 176 | Soldier: SoldierSpawnEntityData @160 |
| SoundAreaEntityData | class | GameEntityData | 208 | MinY: Float32 @128, FadeWidth: Float32 @132, AreaType: SoundAreaType @136, Height: Float32 @140, OuterGeometry: AreaGeometryEntityData[] @144, Geometry: AreaGeometryEntityData[] @164, BigWorld: BigWorldSettingsAsset[] @184, Sound: SoundTemplate @204 |
| SpawnEntityData | class | GameEntityData | 160 | SpawnDelay: Float32 @128, InitializedSpawnDelay: Float32 @132, Team: TeamId @136, Amount: Uint32 @140, FakeImmortal: Boolean @144, Immortal: Boolean @145, AllowMulipleSpawn: Boolean @146 |
| SpawnableEffectComponentData | class | ComponentData | 160 | SpawnProbabilityFactor: Float32 @128, LinkToHealthState: Int32 @132, Effects: EffectAsset[] @136 |
| SpawnableMeshComponentData | class | ComponentData | 160 | PartName: String @128, PhysicsData: PhysicsEntityData @140, Mesh: MeshAsset @144, SpawnAsDestoyed: Boolean @148 |
| SpectateEntityData | class | GameEntityData | 176 | VehicleCameraIndex: Uint32 @128, CapturePointCameraIndex: Uint32 @132, AirVehicleCameraIndex: Uint32 @136, SoldierCameraIndex: Uint32 @140, Mode: SpecateMode @144, Target: SpecateTarget @148, Cameras: TargetCameraData[] @152, UseInHardcoreMode: Boolean @172, ShouldTargetControllable: Boolean @173 |
| SpeedEventGateEntityData | class | GameEntityData | 144 | MaxSpeed: Float32 @128, MinSpeed: Float32 @132 |
| SquadBaseEntityData | class | GameEntityData | 128 | - |
| SquadDebugMenuEntityData | class | GameEntityData | 128 | - |
| SquadEntityData | class | SquadBaseEntityData | 176 | SquadId: Int32 @128, BaseReadiness: ReadinessLevel @132, Soldiers: SoldierSpawnEntityData[] @136, Formation: FormationData @156, SquadStartDispersed: Boolean @160 |
| SquadGhostEntityData | class | SquadBaseEntityData | 144 | Squad: SquadEntityData @128 |
| SquadManagerEntityData | class | GameEntityData | 128 | - |
| StatEventTriggerEntityData | class | GameEntityData | 144 | ParamY: StatsCategoryData @128, ParamX: StatsCategoryData @132, StatEvent: StatEvent @136 |
| StaticModelEntityData | class | GamePhysicsEntityData | 352 | BoundingBoxModifier: Vec3 @144, AnimTrees: AnimTreeData[] @160, Animation: AnimationAsset @180, AboutToBeDestroyedTime: Float32 @184, ManyPartsDestroyedPercentage: Float32 @188, VisualTransformModifiers: VisualTransformModifierData[] @192, AnimationVariations: AnimationAsset[] @212, DisablePathfinding: PathfindingDisableType @232, SkinnedMeshTransforms: LinearTransform[] @236, EdgeModelLodBox: BoundingBoxAsset @252, AnimationCullDistance: Float32 @256, EdgeModels: EdgeModelInstances @260, AnimatedMesh: MeshAsset @264, MeshShaderSet: MeshShaderSetAsset @268, ObjectSets: ObjectSet[] @272, PartLinks: PartLinkData[] @292, TreeBanger: TreeBangerEntityData @312, Mesh: MeshAsset @316, DestroyRuinPartPercentage: Float32 @320, LightMapResolution: Uint32 @324, PhysicsMaterialSet: PhysicsMaterialSetAsset @328, StartAnimationOnCreation: Boolean @332, IrradianceVolumeReceiver: Boolean @333, LoopAnimation: Boolean @334, DoNotCastShadow: Boolean @335, LightMapEnable: Boolean @336 |
| StaticModelGroupEntityData | class | GroupEntityData | 176 | - |
| StaticModelTriggerEntityData | class | TriggerEventEntityData | 176 | PartCountInPerCent: Float32 @144, PartCount: Uint32 @148, StaticModel: ReferenceObjectData @152, PartName: String @156, GivePointsToDamageGiver: Boolean @168, UseCurrentPartDestroyedOnEnable: Boolean @169 |
| StaticPositionEntityData | class | GameEntityData | 128 | - |
| TacticalObjectiveEntityData | class | GameEntityData | 176 | Team1Timer: Float32 @128, Team2Timer: Float32 @132, BriefingSidTeam2: String @136, MissionSid: String @148, BriefingSidTeam1: String @160 |
| TargetEntityData | class | GamePhysicsEntityData | 144 | - |
| TargetObjectData | class | LogicEntityData | 144 | Area: GeometryEntityData @128, SpawnEntity: SpawnEntityData @132, StrengthType: StrengthType @136, SequenceNumber: Int32 @140 |
| TargetObjectDataGroup | class | LogicEntityData | 160 | TargetObjects: TargetObjectData[] @128 |
| TeamEntityData | class | GameEntityData | 176 | Order_FollowMe: VoiceOverEntityData @128, Order_NeedMedic: VoiceOverEntityData @132, Team: TeamData @136, SquadSpawnCamera: ChaseCameraData @140, Id: TeamId @144, Order_Go: VoiceOverEntityData @148, Order_AttackDefend: VoiceOverEntityData @152, Order_NeedBackup: VoiceOverEntityData @156, Order_NeedAmmo: VoiceOverEntityData @160, Order_NeedPickup: VoiceOverEntityData @164 |
| TeamFilterEntityData | class | GameEntityData | 144 | AllowTeam: TeamId @128 |
| TerrainEntityData | class | GamePhysicsEntityData | 160 | OutsideTerrainMaterial: String @144, TerrainAsset: TerrainData @156 |
| TestEntityData | class | GameEntityData | 128 | - |
| TicketCounterEntityData | class | GameEntityData | 176 | TicketLossPerMinPointCountScale: Int32 @128, TicketPercentages: TicketCountPercentage[] @132, InitialTicketCount: Int32 @148, TicketLossStart: Int32 @152, TicketLossWhenLostAll: Int32 @156, TeamId: TeamId @160, TicketLossPerMin: Int32 @164, TicketLossWhenLostAllBases: Int32 @168, HaltTicketLossOnEqualPointCount: Boolean @172, SetBestSquadSpawner: Boolean @173 |
| TooltipEntityData | class | GameEntityData | 144 | TooltipSid: String @128, EntryInputAction: EntryInputActionEnum @140 |
| TracerDartEntityData | class | MeshProjectileEntityData | 224 | Gravity: Float32 @208, TransmissionEffect: EffectAsset @212 |
| TrackComponentData | class | SkinnedMeshComponentData | 176 | ScrollFactor: Float32 @160 |
| TrackWheelComponentData | class | WheelComponentData | 208 | BoneName: String @192 |
| TreeBangerEntityData | class | BangerEntityBaseData | 240 | SpawnEffects: EffectAsset[] @192, PartLinks: PartLinkData[] @212 |
| TriggerEntityData | class | LogicUtilEntityData | 144 | Delay: Float32 @128, RunOnce: Boolean @132 |
| TriggerEventEntityData | class | TriggerEntityData | 144 | - |
| UnderFireTriggerEntityData | class | TriggerEntityData | 192 | TriggerOnMaterialOnly: String @144, TrackEntity: LogicEntityData @156, TriggeredBy: AreaTriggerInclude @160, Radius: Float32 @164, Area: GeometryEntityData @168, CoolDownTime: Float32 @172, DisableIfTrackEntityIsLost: Boolean @176 |
| VehicleComponentData | class | PartComponentData | 240 | WaterEffects: VehicleWaterEffectData[] @176, WaterStreakEffects: VehicleWaterEffectData[] @196, TriggerGroundEffectImpulse: Float32 @216, VehicleConfig: VehicleConfigData @220, GearboxConfig: GearboxConfigData @224, AlwaysFullThrottle: Boolean @228 |
| VehicleEntityData | class | ControllableEntityData | 608 | ExplosionOffset: Vec3 @192, PreExplosionEffectPosition: Vec3 @208, CriticallyDamagedEffectPosition: Vec3 @224, CriticalDamageTime: Float32 @240, WaterDamageOffset: Float32 @244, PreExplosionTime: Float32 @248, UpsideDownDamageDelay: Float32 @252, BelowWaterDamageDelay: Float32 @256, UpsideDownAngle: Float32 @260, UpsideDownDamage: Float32 @264, NeedMoreHealthPercentage: Float32 @268, ImpulseThreshold: Float32 @272, ImpulseScale: Float32 @276, Rumble: VehicleRumbleData @280, KillSoldierCollisionSpeedThreshold: Float32 @324, ExitSpeedThreshold: Float32 @328, MPMode: MPModeData @332, WaterDamage: Float32 @336, RearHealthZone: VehicleHealthZoneData @352, FrontHealthZone: VehicleHealthZoneData @400, LeftHealthZone: VehicleHealthZoneData @448, MeshShaderSet: MeshShaderSetAsset @496, Mesh: CompositeMeshAsset @500, Explosion: ExplosionEntityData @504, PreExplosionEffect: EffectAsset @508, CriticallyDamagedEffect: EffectAsset @512, HudData: VehicleHudData @516, MaxPlayersInVehicle: Int32 @524, NameSid: String @528, RightHealthZone: VehicleHealthZoneData @544, MinSpeedForMineActivation: Float32 @592, ThrowOutUpsideDownPassengers: Boolean @596, ThrowOutSoldierInsideOnWaterDamage: Boolean @597, SelfDestructIfAbandoned: Boolean @598, HealthZonesShareDamage: Boolean @599, IsAffectedByEMP: Boolean @600, EnableGroundmapLighting: Boolean @601 |
| VehicleExitPointComponentData | class | ComponentData | 128 | - |
| VehicleProjectileEntityData | class | ProjectileEntityData | 192 | EnterDelay: Float32 @176, Vehicle: VehicleAsset @180 |
| VehicleSpawnEntityData | class | SpawnEntityData | 272 | KeepAliveRadius: Float32 @160, WreckDuration: Float32 @164, BotBailOutDelay: Float32 @168, TimeUntilAbandonedIsDestroyed: Float32 @172, BotBailWhenHealthBelow: Float32 @176, LodDistance: Float32 @180, EnterRestriction: EntryEnterRestriction @184, MeshShaderSetNumber: Uint32 @188, MaxVehicles: Uint32 @192, MaxVehiclesPerMap: Uint32 @196, TimeUntilAbandoned: Float32 @200, RotationYaw: Float32 @204, RotationPitch: Float32 @208, TakeControlEntryIndex: Uint32 @212, Vehicle: VehicleAsset @216, Vehicles: TeamVehicleAsset[] @220, SpawnAreaRadius: Float32 @236, RespawnRange: Float32 @240, Brake: Float32 @244, RotationRoll: Float32 @248, Throttle: Float32 @252, EffectedByImpulse: Boolean @256, SendWeaponEvents: Boolean @257, OnlySendEventForHumanPlayers: Boolean @258, SetTeamOnSpawn: Boolean @259, ApplyDamageToAbandondedVehicles: Boolean @260, AutoSpawn: Boolean @261, SnapToSurface: Boolean @262, DestroyVehiclesOnDisable: Boolean @263, SpawnVehicleFromBeginning: Boolean @264 |
| VehicleStateTriggerEntityData | class | TriggerEntityData | 208 | HealthToCheck: Uint32 @144, HealthToSet: Uint32 @148, VehiclesToExclude: VehicleSpawnEntityData[] @152, EffectRadius: Float32 @172, BulletsAmount: Uint32 @176, VehicleToCheck: VehicleSpawnEntityData @180, LockAmount: Float32 @184, Restriction: EnterVehicleRestriction @188, SeatSpecificToEnter: Uint32 @192, EffectBangers: Boolean @196, CheckHealthOnAndAbove: Boolean @197, TriggerCriticalOnHealthSet: Boolean @198, EffectVehicles: Boolean @199 |
| VoiceOverEntityData | class | GameEntityData | 176 | Radius: Float32 @128, MinDelayTime: Float32 @132, StartVoDelayTime: Float32 @136, VoiceOverLines: VoiceOverLineAsset[] @140, Team: TeamId @160, Subtitle: SubtitleDataTree @164, OverRadio: Boolean @168, UseSoldierGestures: Boolean @169, RunOnce: Boolean @170 |
| WeaponCallbackEntityData | class | GameEntityData | 128 | - |
| WeaponComponentData | class | PartComponentData | 224 | ProjectileSpawnOffset: Vec3 @176, Effects: WeaponEffectsData @192, AIData: WeaponData @196, ImpulseStrength: Float32 @200, SpecializationType: PersistentSpecializationType @204, AnimTree: AnimTreeDataAsset @208, WeaponMesh: MeshAsset @212, WeaponFiring: WeaponFiringData @216, Skeleton: SkeletonAsset @220 |
| WeaponInputRouterComponentData | class | ComponentData | 144 | RotationCount: Uint32 @128, MaxFireRate: Float32 @132 |
| WeaponPickupEntityData | class | PickupEntityData | 192 | Soldier: SoldierSpawnBaseEntityData @160, Weapons: WeaponPickupData[] @164, UseForPersistence: Boolean @180 |
| WeatherStateEntityData | class | GameEntityData | 144 | FadeTime: Float32 @128, State: Uint32 @132, DefaultState: Uint32 @136, StartTransitionOnSpawn: Boolean @140, TriggerEventFromClient: Boolean @141 |
| WheelComponentData | class | PartComponentData | 192 | EffectClampVelocity: Float32 @176, PhysicsType: WheelPhysicsType @180, Config: WheelConfigData @184 |
| WingComponentData | class | PartComponentData | 192 | Config: WingPhysicsData @176 |
| WorldInteractionComponentData | class | ComponentData | 144 | MiddlegrowthSpeedThreshold: Float32 @128 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: GameSharedResources classes below EntityData 188]

### RTTI-GSR-4: Weapons, firing and damage

Status: **verified** · Variants: PC

The weapon's data outside the entities: firing (`FiringFunctionData` and its parts), the gun sway (`GunSwayData`:
stand, crouch, prone and the six transitions), the effects per weapon kind (`...WeaponEffectsData`), ammunition,
zoom, overheating, the projectiles' data, damage. `engine/weapons.md` is what the game does with them.

<!-- typeinfo:gsr-weapons -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AmmoBoxWeaponEffectsData | class | WeaponEffectsData | 28 | - |
| ArmamentData | class | DataContainer | 24 | TurretData: TurretData @12, ThreatLength: Int32 @16, IsAntiAircraft: Boolean @20 |
| ArtilleryStrikeFiringEffectsData | class | WeaponEffectsData | 100 | StrikeCameraFov: Float32 @28, FireCameraTime: Float32 @32, AimingCameraFov: Float32 @36, DelayBeforeAimingCamera: Float32 @40, ValidMaxDistance: Float32 @44, ValidMaxAngle: Float32 @48, StrikeCameraTime: Float32 @52, ValidMinDistance: Float32 @56, StrikeCameraOffset: Float32 @60, SpawnHeight: Float32 @64, MaxRandomSpawnHeight: Float32 @68, MaxStrikeDistance: Float32 @72, StrikeRadius: Float32 @76, StrikeCameraHeight: Float32 @80, AimingCameraOffset: Float32 @84, Camera: TargetCameraData @88, AimingCameraHeight: Float32 @92, EnableCameraRotation: Boolean @96, EnableProjectileTrails: Boolean @97 |
| BinocularsWeaponEffectsData | class | WeaponEffectsData | 36 | MaxDistance: Float32 @28, FOV: Float32 @32 |
| CallbackWeaponEffectsData | class | WeaponEffectsData | 40 | CallbackEntityName: String @28 |
| CameraRecoilData | class | DataContainer | 24 | SpringMinThresholdAngle: Float32 @12, SpringDamping: Float32 @16, SpringConstant: Float32 @20 |
| DefibrillatorWeaponEffectsData | class | WeaponEffectsData | 36 | Range: Float32 @28, FocusConeAngle: Float32 @32 |
| DetonatedWeaponEffectsData | class | WeaponEffectsData | 36 | Range: Float32 @28, LockingController: LockingControllerData @32 |
| FiringFunctionData | class | DataContainer | 496 | FireLogic: FireLogicData @12, Shot: ShotConfigData @160, ReflectionSound: SoundAsset @240, SelfHealTimeWhenDeployed: Float32 @244, OverHeat: OverHeatData @256, Ammo: AmmoConfigData @368, OutOfAmmoSoundEffect: SoundAsset @404, FireEffects1p: FireEffectData[] @408, WeaponDispersion: SoldierWeaponDispersion @424, Dispersion: FiringDispersionData[] @444, ProjectileFireSound: SoundAsset @460, FireSoundEffect: EffectAsset @464, FireEffects3p: FireEffectData[] @468, IsCounterMeasureWeapon: Boolean @484, LoopingFireSound: Boolean @485, UsePrimaryAmmo: Boolean @486, UnlimitedAmmoForAI: Boolean @487 |
| GunSwayData | class | WeaponSwayData | 828 | Modifiers: GunSwayModifierData[] @12, DeviationScaleFactorZoom: Float32 @32, StandToCrouch: GunSwayStanceTransition @36, SuppressionModifier: GunSwayStanceZoomModifierData @60, GameplayDeviationScaleFactorNoZoom: Float32 @92, CameraRecoil: CameraRecoilData @96, GameplayDeviationScaleFactorZoom: Float32 @100, DeviationScaleFactorNoZoom: Float32 @104, Prone: GunSwayCrouchProneData @108, ProneToCrouch: GunSwayStanceTransition @268, Stand: GunSwayStandData @292, Crouch: GunSwayCrouchProneData @572, CrouchToStand: GunSwayStanceTransition @732, StandToProne: GunSwayStanceTransition @756, ProneToStand: GunSwayStanceTransition @780, CrouchToProne: GunSwayStanceTransition @804 |
| GunSwayModifierData | class | Asset | 220 | CrouchNoZoomModifier: GunSwayStanceZoomModifierData @24, ProneZoomModifier: GunSwayStanceZoomModifierData @56, ProneNoZoomModifier: GunSwayStanceZoomModifierData @88, StandZoomModifier: GunSwayStanceZoomModifierData @120, StandNoZoomModifier: GunSwayStanceZoomModifierData @152, CrouchZoomModifier: GunSwayStanceZoomModifierData @184, OnlyInSupportedShooting: Boolean @216 |
| LaserDesignatorEffectsData | class | LockingWeaponEffectsData | 56 | BombWarnTime: Float32 @40, BomberSound: SoundAsset @44, PostLockTime: Float32 @48, BomberTime: Float32 @52 |
| LockingWeaponEffectsData | class | WeaponEffectsData | 40 | WarnLock: WarnTarget @28, LockingController: LockingControllerData @32, IsHoming: Boolean @36, IsGuided: Boolean @37 |
| MaterialRelationDamageData | class | MaterialRelationPropertyData | 44 | DamagePenetrationMultiplier: Float32 @16, DamageProtectionThreshold: Float32 @20, ExplosionCoverDamageModifier: Float32 @24, CollisionDamageMultiplier: Float32 @28, CollisionDamageThreshold: Float32 @32, DamageProtectionMultiplier: Float32 @36, InflictsDemolitionDamage: Boolean @40, NeverPenetrate: Boolean @41 |
| MedicBagWeaponEffectsData | class | WeaponEffectsData | 28 | - |
| MedkitWeaponEffectsData | class | WeaponEffectsData | 28 | - |
| MeleeWeaponEffectsData | class | WeaponEffectsData | 48 | StickToTargetDelay: Float32 @28, KeepStickyAimTime: Float32 @32, AttackMaterial: String @36 |
| MortarStrikeFiringEffectsData | class | WeaponEffectsData | 44 | StrikeRadius: Float32 @28, MaxRandomSpawnHeight: Float32 @32, HoldingTolerance: Float32 @36, MaxStrikeDistance: Float32 @40 |
| PersistenceWeaponData | class | Asset | 44 | Weapons: PersistentWeapon[] @24 |
| PersistentWeapon | class | Asset | 84 | Licenses: String @24, Dependency: PersistentWeapon @36, Slot: Uint32 @40, AdditionalLicenses: String @44, Cost: Uint32 @56, WeaponType: PersistentWeaponType @60, Id: String @64, BitIndex: Int32 @76, Collectible: Boolean @80 |
| PowerToolWeaponEffectsData | class | WeaponEffectsData | 52 | DamageSound: SoundAsset @28, RepairMaterial: String @32, RepairSound: SoundAsset @44, RepairCompletedSound: SoundAsset @48 |
| RepairToolWeaponEffectsData | class | PowerToolWeaponEffectsData | 56 | RechargeSpeed: Float32 @52 |
| SoldierWeaponAsset | class | ObjectBlueprint | 88 | - |
| SoldierWeaponData | class | GameObjectData | 304 | ZoomRenderFov: Float32 @48, AltDeployId: Int32 @52, AnimBaseSet: WeaponAnimBaseSetEnum @56, RenderFov: Float32 @60, WeaponAssetName: String @64, SpecializationWeapons: SoldierWeaponAsset[] @76, MeshShaderSetNumberOverride: Int32 @96, Persistence: PersistentWeapon @100, Specializations: WeaponSpecializationLink[] @104, StreamGroup1p: StreamGroupData @120, Effects: WeaponEffectsData @124, FirstPersonCamera: FirstPersonCameraData @128, WeaponClass: WeaponClassEnum @132, WeaponStates: WeaponStateData[] @136, WeaponFiring: WeaponFiringData @152, AimingController: SoldierAimingSimulationData @156, Hud: HudData @160, AIData: WeaponData @240, PickupSettings: PickupSettingsData @256, LowerOnOwnTeam: Boolean @288, HideWhenOutOfAmmo: Boolean @289, SwitchToPrimaryWhenOutOfAmmo: Boolean @290, SwitchToPreviousWeaponAfterFire: Boolean @291, AllowSwitchingToWeaponOutOfAmmo: Boolean @292, LoadOnDemand: Boolean @293 |
| SoldierWeaponOverrideData | class | DataContainer | 32 | Overrides: WeaponOverrideData[] @12 |
| VOWeapon | class | DataContainer | 24 | Name: String @12 |
| VOWeaponParam | class | VOParam | 32 | Weapon: VOWeapon @28 |
| WeaponData | class | DataContainer | 144 | AimOrigin: AimOrigin @12, AngleToTargetSnap: Float32 @16, MinimumAccuracyBeforeFiring: Float32 @20, GravityModifier: Float32 @24, LostTargetBurstCoolDownModifier: Float32 @28, MaxBurstCoolDownTime: Float32 @32, AimAngularVelocity: Float32 @36, AimTransitionFireDelay: Float32 @40, OuterInaccurateDistance: Float32 @44, InnerInaccurateDistance: Float32 @48, FinalAccuracyPercentage: Float32 @52, StartAccuracyPercentage: Float32 @56, SweepType: SweepSelectionType @60, AdditionalMinimumAccuracyAtMinRange: Float32 @64, SweepTime: Float32 @68, SweepDistance: Float32 @72, MinBurstCoolDownTime: Float32 @76, ReloadBehavior: ReloadBehavior @80, MinExtraReloadTime: Float32 @84, MaxExtraReloadTime: Float32 @88, OptimalRangePercentage: Float32 @92, Strengths: Strengths @96, MinRange: Float32 @112, MaxRange: Float32 @116, BurstLimitNear: Int32 @120, BurstLimitFar: Int32 @124, IsAiAllowed: Boolean @128, IsMelee: Boolean @129, InaccuracyFollowsTarget: Boolean @130, CanSuppress: Boolean @131, Indirect: Boolean @132, UseInaccuracyAtCloseRange: Boolean @133, UseZoom: Boolean @134, AimHighIndirectPath: Boolean @135, AllowedAgainstImmortals: Boolean @136, AimWhileReloading: Boolean @137, IsStandStill: Boolean @138, CanAltAttack: Boolean @139, SweepFirstAttack: Boolean @140 |
| WeaponEffectsData | class | DataContainer | 28 | ActiveSpotConeAngle: Float32 @12, OrderConeAngle: Float32 @16, FocusConeAngle: Float32 @20, CanSpotAndOrder: Boolean @24 |
| WeaponFiringData | class | DataContainer | 76 | WeaponSway: WeaponSwayData @12, SpottingDistance: Float32 @16, VOTypes: VOWeapon[] @20, AiDamageModifier: Float32 @40, Rumble: RumbleFiringData @44, DeployTime: Float32 @56, ReactivateCooldownTime: Float32 @60, PrimaryFire: FiringFunctionData @64, AbortReloadOnSprint: Boolean @68, AutoSpotOnZoom: Boolean @69, UseAutoAiming: Boolean @70, AutoSpotOnFire: Boolean @71, FireOnDeploy: Boolean @72, InflictSelfDamage: Boolean @73 |
| WeaponLagSpringEffectData | class | DataContainer | 208 | LandForceZoomModifier: Float32 @12, LandForces: WeaponLagEffectForceData @16, PoseDownForces: WeaponLagEffectForceData @48, PoseUpForces: WeaponLagEffectForceData @80, RotationSprings: WeaponLagEffectSpringVector @112, OffsetSprings: WeaponLagEffectSpringVector @136, JumpForceZoomModifier: Float32 @160, JumpForces: WeaponLagEffectForceData @176 |
| WeaponOverrideData | class | DataContainer | 32 | Values: WeaponOverrideValue[] @12, Data: DataContainer @28 |
| WeaponSwayData | class | DataContainer | 12 | - |
| WeaponSwitchingData | class | DataContainer | 32 | SwitchToGadgetCooldownTime: Float32 @12, SwitchMap: WeaponSwitchingMapData[] @16 |
| WeaponUnlockVisualData | class | Asset | 44 | AnimTreeReplacement1p: AnimTreeReplacementData @24, AnimTreeReplacement3p: AnimTreeReplacementData @28, Mesh3p: SkinnedMeshAsset @32, Mesh1p: SkinnedMeshAsset @36, MeshZoom1p: SkinnedMeshAsset @40 |
| ZoomLevelData | class | DataContainer | 116 | FadeFromBlackDuration: Float32 @12, TransitionTime: Float32 @16, FovDelayTime: Float32 @20, StartFadeToBlackAtTime: Float32 @24, FadeToBlackDuration: Float32 @28, StartFadeFromBlackAtTime: Float32 @32, FovTransitionTime: Float32 @36, SpottingConeFov: Float32 @40, SpottingConeLength: Float32 @44, SpottingConeFalloff: Float32 @48, ForegroundBlurFilter: BlurFilter @52, ForegroundBlurFilterDeviation: Float32 @56, ForegroundBlurViewDistance: Float32 @60, SwayPitchMultiplier: Float32 @64, SwayYawMultiplier: Float32 @68, FieldOfView: Float32 @72, LookSpeedMultiplier: Float32 @76, SprintLookSpeedMultiplier: Float32 @80, MoveSpeedMultiplier: Float32 @84, RecoilFovMultiplier: Float32 @88, CameraImpulseMultiplier: Float32 @92, TimePitchMultiplier: Float32 @96, DispersionMultiplier: Float32 @100, TimeYawMultiplier: Float32 @104, RecoilMultiplier: Float32 @108, FadeToBlackInZoomTransition: Boolean @112, UseFovSpecialisation: Boolean @113 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27]

### RTTI-GSR-5: AI, cover, squads and voice-over

Status: **verified** · Variants: PC

The bots' data: behaviours and their urgency, cover, the path data's generator settings, the sensing limits, the
readiness levels, the squads, the voice-over rows and parameters. RTTI-DBX-2 lists the fields of these classes the
game does not read.

<!-- typeinfo:gsr-ai -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AIAimingConstraintsData | class | DataContainer | 28 | MinPitch: Float32 @12, MaxPitch: Float32 @16, MinYaw: Float32 @20, MaxYaw: Float32 @24 |
| AISettingsData | class | Asset | 156 | TimingConstants: TimingConstantData @24, LookConstants: LookConstantData @28, DebugConstants: DebugConstantData @32, EnvironmentConstants: EnvironmentConstantData @36, FollowConstants: FollowConstantData @40, PositionEvaluationConstants: PositionEvaluationConstantData @44, DefaultSensingTemplate: SensingTemplateData @48, DefaultBehaviourTemplate: BehaviourTemplateData @52, DefaultVehicleType: AIVehicleBehaviourData @56, KynapseConstants: KynapseConstantData @60, TraversalConstants: TraversalConstantData @64, ReadinessLevels: ReadinessLevels @68, SearchAndDestroyConstants: SearchAndDestroyConstantData @104, NavigationConstants: NavigationConstantData @108, SoldierControlConstants: SoldierControlConstantData @112, CarControlConstants: CarControlConstantData @116, SensingConstants: SensingConstantData @120, SoundEnvironmentConstants: SoundEnvironmentConstantData @124, CoverConstants: CoverConstantData @128, TargetEvaluationConstants: TargetEvaluationConstantData @132, VOConstants: VOConstantData @136, CombatConstants: CombatConstantData @140, TankControlConstants: TankControlConstantData @144, HelicopterControlConstants: HelicopterControlConstantData @148, TurretControlConstants: TurretControlConstantData @152 |
| AIStateAnimationData | class | DataContainer | 12 | - |
| AIVehicleBehaviourData | class | DataContainer | 80 | Goals: String[] @12, Navigation: NavigationType @28, PathfindingType: PathfindingType @32, Name: String @36, Controls: String[] @48, Behaviours: BehaviourMappingData[] @64 |
| AIVehicleTypes | class | Asset | 84 | SquadBehaviours: BehaviourData[] @24, Goals: BehaviourData[] @44, VehicleTypes: AIVehicleBehaviourData[] @64 |
| AnimatedWaypointData | class | WaypointData | 96 | AnimationInMovement: Vec3 @48, AnimationOut: AnimationAsset @64, Loop: AnimationAsset @68, AnimationInEndPose: SoldierAnimationPose @72, AnimationIn: AnimationAsset @76, AnimationIndex: Int32 @80, AnimationInEndAngle: Float32 @84, AnimationInDuration: Float32 @88 |
| BehaviourData | class | UrgencyUserData | 64 | Name: String @52 |
| BehaviourTemplateData | class | Asset | 188 | ObstructorMinMoveDistance: Float32 @24, ObstructorRespondDistance: Float32 @28, ObstructorMaxMoveDistance: Float32 @32, TargetStrengthFactor: Float32 @36, TargetDistanceFactor: Float32 @40, MeleePushForce: Float32 @44, MeleeDamage: Float32 @48, MeleeDelay: Float32 @52, FrontLineDistance: Int32 @56, MeleeAnimationId: Int32 @60, AccuracyIncreaseTime: Float32 @64, AccuracyModifier: Float32 @68, SuppressIndirectProbability: Float32 @72, ReloadBelowPercent: Float32 @76, MaxSuppressTime: Float32 @80, TargetIsAttackingFactor: Float32 @84, TargetDangerFactor: Float32 @88, TargetIsMarkedFactor: Float32 @92, RPGSkill: RpgSkill @96, SoldierMovementVolumeOverride: Float32 @100, MeleeStartDistance: Float32 @104, MinAttackBursts: Uint32 @108, ExplosiveObjectChance: Float32 @112, MaxAttackBursts: Uint32 @116, AltAttackProbability: Float32 @120, AttackUrgencyModifier: Float32 @124, CrouchProbability: Float32 @128, StandProbability: Float32 @132, StunnedWhenHealthPercentageBelow: Float32 @136, InvestigateStopDelay: Float32 @140, TurnAgainstTargetDistance: Float32 @144, ReadinessReactionTimeMultiplier: Float32 @148, LowHealthThreshold: Float32 @152, ExplosionEmergentTakeCoverProbability: Float32 @156, UseSameCoverModifier: Float32 @160, RollOutOfCoverProbability: Float32 @164, ForceSameCoverModifier: Float32 @168, BaseTimeInCover: Float32 @172, MinEnemyDistanceToUseCover: Float32 @176, SuppressedTimeAdd: Float32 @180, AllowSleep: Boolean @184, AlwaysUpdate: Boolean @185, LeaveVehicleOnSearchAndDestroyOrder: Boolean @186, StunnedInVehicles: Boolean @187 |
| BotPriorityConfigData | class | DataContainer | 48 | DebugColor: Vec3 @16, AngleOffset: Float32 @32, Period: Int32 @36, Distance: Float32 @40, AppliesToSensing: Boolean @44, AppliesToExecution: Boolean @45, AppliesToDecisions: Boolean @46 |
| CoverConstantData | class | DataContainer | 204 | FrontlineEvaluationWeight: Float32 @12, FrontlineDistanceCap: Uint32 @16, ReevaluationTime: Float32 @20, ThreatRadius: Float32 @24, CoverSelectionMethod: CoverSelectionMethod @28, SwitchCoverUrgencyThreshold: Float32 @32, PositionEvaluationWeight: Float32 @36, IntermediateCoverSlotModifier: Float32 @40, DistanceEvaluationWeight: Float32 @44, EnemiesEvaluationWeight: Float32 @48, FitnessEvaluationWeight: Float32 @52, CoverFailIgnoreTime: Float32 @56, FinishDueToSeeingDelay: Float32 @60, PathfindingTypesUsingCovers: PathfindingSettings[] @64, HideCooldownMax: Float32 @84, HideCooldownMin: Float32 @88, MinDistanceToMoveStanding: Float32 @92, InCoverDeviation: Float32 @96, InCoverDistance: Float32 @100, BaseTimeHealthModifier: Float32 @104, MinAttackableEnemyDistance: Float32 @108, BaseTimeUrgencyModifier: Float32 @112, StandCoverModifier: Float32 @116, SearchRadius: Float32 @120, RaysPerDetailSearch: Int32 @124, BaseHeight: Float32 @128, MaxCoverLength: Float32 @132, StandHeight: Float32 @136, SlotSpacing: Float32 @140, SlotSize: Float32 @144, SlotEdgeDistance: Float32 @148, RaysPerSlot: Float32 @152, MinimumOpenCoverWidth: Float32 @156, SearchDirections: Int32 @160, MaxNodesForProgression: Int32 @164, DistanceStep: Float32 @168, MovedResetDistance: Float32 @172, CrouchCoverModifier: Float32 @176, MaxCoversInMemory: Int32 @180, SearchDirectionsPerTimeSlice: Int32 @184, ForcedDirections: Int32 @188, SearchRange: Float32 @192, HitSearchRange: Float32 @196, UseRaycastCache: Boolean @200, ForceRegenerateDestroyedCovers: Boolean @201 |
| CoverData | class | DataContainer | 80 | TopRight: Vec3 @16, BottomLeft: Vec3 @32, RightEdge: CoverEdge @48, LeftEdge: CoverEdge @52, Material: CoverType @56, Right: CoverData @60, Left: CoverData @64 |
| EdgeModelBuilderSettings | class | Asset | 84 | MinSpacing: Float32 @24, MaxSpacing: Float32 @28, CornerPartExclusionPatterns: String[] @32, VolumeMap: EdgeModelVolumeMapItemData[] @48, PartExclusionPatterns: String[] @64, EnableEdgeModels: Boolean @80 |
| EdgeModelGenerationSettings | class | DataContainer | 36 | OrientationVolumes: EdgeModelVolumeAsset @12, SharedSettings: EdgeModelBuilderSettings @16, VolumeMap: EdgeModelVolumeMapItemData[] @20 |
| EdgeModelInstances | class | DataContainer | 64 | Links: EdgeModelLinkData[] @12, MultiLinks: EdgeModelMultiLinkData[] @28, Meshes: EdgeModelMeshData[] @44 |
| EdgeModelMeshData | class | Asset | 28 | Mesh: RigidMeshAsset @24 |
| EdgeModelVolumeAsset | class | Asset | 48 | RootNode: String @24, SourceFile: FileRef @36 |
| EdgeModelVolumeExportResult | class | DataContainer | 96 | StripCutterGroup: EdgeModelVolumeGroup @12, ExcludeGroup: EdgeModelVolumeGroup @40, IncludeGroup: EdgeModelVolumeGroup @68 |
| FormationData | class | DataContainer | 40 | Name: String @12, Positions: Vec3[] @24 |
| FormationHolder | class | Asset | 44 | Formations: FormationData[] @24 |
| KynapseConstantData | class | DataContainer | 28 | MaxMobileVehicles: Int32 @12, KynapseLicenseServerName: String @16 |
| NavigationConstantData | class | DataContainer | 164 | MinSpeedForMovingAnimationStart: Float32 @12, MaxAnimationDirectionDifference: Float32 @16, MovementAnimationPrecisionRatio: Float32 @20, HelicopterWaypointInterpolationDistance: Float32 @24, AllowPreSpecialMovementDistance: Float32 @28, GraphStreamCollectBoxSamplingStep: Float32 @32, AvoidVehicleMoveDistance: Float32 @36, GraphStreamCollectBoxSizeFactor: Float32 @40, FailureRetryWaitTime: Float32 @44, GraphStreamCacheTime: Float32 @48, MoveToValidWPRadius: Float32 @52, AerialRandomDestinationHeight: Float32 @56, AvoidVehicleDistance: Float32 @60, AvoidVehicleMoveRadius: Float32 @64, AvoidVehicleSize: Float32 @68, MinimumHelicopterHeight: Float32 @72, HelicopterPathfindingSearchSteps: Int32 @76, HelicopterPathRadius: Float32 @80, ValidAerialPositionHeight: Float32 @84, HelicopterRecalculateRadius: Float32 @88, RandomDestinationHeightCheck: Float32 @92, RoadCostModifier: Float32 @96, LpfSectorSize: Float32 @100, LpfStreamingMethod: LpfStreamingMethod @104, WaterCostModifier: Float32 @108, SpeedThresholdForBeingStuck: Float32 @112, MaxPathObjects: Uint32 @116, TeleportForbidSector: Float32 @120, TimeStuckBeforeTeleport: Float32 @124, LocalPathfindingLoadRadius: Float32 @128, FindNearestSearchRadius: Float32 @132, FindNearestDistMax: Float32 @136, ForbiddenPathObjectStreamingDistance: Float32 @140, FindNearestSearchMaxNodes: Int32 @144, FindNearestInsideAIMeshMethod: FindNearestMethod @148, MaxNodesToTrace: Uint32 @152, MaxDynamicObjects: Uint32 @156, IgnoreKynapseSpeed: Boolean @160, RejectBadGoals: Boolean @161, UseTraceLineFallback: Boolean @162, UseTimeslicedSearch: Boolean @163 |
| OrderReadiness | class | UrgencyUserData | 72 | Minimum: ReadinessState @52, Initial: ReadinessState @56, OrderType: String @60 |
| OrderSettings | class | Asset | 44 | Orders: OrderReadiness[] @24 |
| PathdataSectorOffsets | class | Asset | 32 | ZOffset: Int32 @24, XOffset: Int32 @28 |
| PathfindingSettings | class | Asset | 120 | MaxPitch: Float32 @24, VerticalBoxSize: Float32 @28, EntityHeight: Float32 @32, EdgeRadius: Float32 @36, OverConnectionRatio: Float32 @40, DetailedZoneSettings: PathfindingZoneSettings @44, ForbidHeight: Float32 @48, WaterLevelAdjustment: Float32 @52, CollisionModel: PathdataCollisionModel @56, OutputType: PathfindingType @60, Generator: PathdataGenerator @64, NbSectors: Int32 @68, StepMax: Float32 @72, GroundSlopeMin: Float32 @76, MaxDeltaHeight: Float32 @80, StepLength: Float32 @84, HoleMax: Float32 @88, GroundSlopeMax: Float32 @92, VerticalPitch: Float32 @96, GraphAccuracy: Float32 @100, Pitch: Float32 @104, UseBoxApproximation: Boolean @108, BuildRoads: Boolean @109, UseForbid: Boolean @110, UseExclusions: Boolean @111, BuildCompositeShapes: Boolean @112, VehicleSeeds: Boolean @113, InfantrySeeds: Boolean @114, ForbidCollidingEdge: Boolean @115, ForbidWater: Boolean @116, BuildBridges: Boolean @117 |
| PathfindingTerrainData | class | DataContainer | 12 | - |
| PathfindingType | class | Asset | 76 | MaxGaps: Int32 @24, DiagramHalfWidth: Float32 @28, DistEdgeMax: Float32 @32, AccidentDetection: PathfindingAccidentDetection @36, CanGoMaxEntityRadius: Float32 @40, EntityRadius: Float32 @44, OutputName: String @48, MaxGraphHandles: Int32 @60, SubgraphBoxSize: Float32 @64, SubgraphsToLoad: Int32 @68, UseLocalPathfinding: Boolean @72 |
| PathfindingZoneSettings | class | DataContainer | 24 | NbSectors: Int32 @12, GraphAccuracy: Float32 @16, Pitch: Float32 @20 |
| SensingConstantData | class | DataContainer | 144 | MotionSizeFactor: Float32 @12, MotionSpeedFactor: Float32 @16, MotionPeripheralityFactor: Float32 @20, MaxSize: Float32 @24, MaxSpeed: Float32 @28, BackgroundLoudnessFactor: Float32 @32, MotionDistanceFactor: Float32 @36, SpawnOutOfLOSTraceDistance: Float32 @40, SpawnOutOfLOSRaycastDistance: Float32 @44, SpawnOutOfLOSRadius: Float32 @48, SmokeSphereRadius: Float32 @52, SmokeSphereDuration: Float32 @56, SpawnOutOfLOSRaycastHeight: Float32 @60, MaxVolume: Float32 @64, LineOfSightTestTime: Float64 @72, ExplosionInterestRange: Float32 @80, MaxRays: Int32 @84, SensingTimeSpan: Float64 @88, FireKeepTime: Float64 @96, AttackerTrackTime: Float64 @104, VisionBoundingBoxSize: Float32 @112, CloseExplosiveImpactDistance: Float32 @116, CloseShellDistance: Float32 @120, ImpactSenseDistance: Float32 @124, VisionHumanBoxOffset: Float32 @128, CloseDistance: Float32 @132, CloseFireImpactDistance: Float32 @136 |
| SensingTemplateData | class | Asset | 160 | RelaxedLimits: SensingLimits @24, MaximumReadinessRaiseDistance: Float32 @52, ReadyLimits: SensingLimits @56, DebugText: String @84, CombatLimits: SensingLimits @96, LostForgetTime: Float64 @128, AwareForgetTime: Float64 @136, TimeUntilUnseenIsLost: Float64 @144, MaximumAlertDistance: Float32 @152, MaximumMergeDistance: Float32 @156 |
| SoldierAimingConstraintsData | class | DataContainer | 28 | SwimmingConstraints: AimingConstraintsData @12 |
| SoldierAimingSimulationData | class | DataContainer | 124 | AimingRange: Float32 @12, PronePose: AimingPoseData @16, AimerType: Uint32 @40, LockAimToTargetSpeed: Float32 @44, CrouchPose: AimingPoseData @48, ZoomType: ZoomTypeEnum @72, ZoomLevels: ZoomLevelData[] @76, StandPose: AimingPoseData @96, HaloAimer: HaloAimerData @120 |
| SquadAsset | class | Asset | 44 | SquadTypes: SquadTypeData[] @24 |
| SquadData | class | DataContainer | 16 | MaxPlayerCount: Uint32 @12 |
| SquadTypeData | class | DataContainer | 48 | Class: SquadClass @12, Readiness: ReadinessState @16, Behaviours: BehaviourSetting[] @20, Name: String @36 |
| UrgencySetData | class | DataContainer | 40 | UrgencyModifiers: UrgencyModifierData[] @12, Name: String @28 |
| UrgencyUserData | class | DataContainer | 52 | ModifierSets: UrgencySetData[] @12, UrgencyModifiers: UrgencyModifierData[] @32, Base: UrgencyUserData @48 |
| VOAsset | class | TreeBase | 44 | Events: VOEvent[] @24 |
| VOConstantData | class | DataContainer | 76 | LoiterActivityDistance: Float32 @12, LoiterHumanActivityTime: Float32 @16, TimeBeforeFirstLookAtPlayerMax: Float32 @20, MinTimeAfterInterludeBeforeLookAtPlayer: Float32 @24, LoiterDistanceSquadToFriendlies: Float32 @28, LoiterAllowedEnemyDistance: Float32 @32, LoiterMemberActivityTime: Float32 @36, LoiterPauseTime: Float32 @40, MaxEnemyBehindPlayerHeightDiff: Float32 @44, MaxPlayerHurtDistance: Float32 @48, EnemyBehindPlayerRadius: Float32 @52, EnemyBehindPlayerArc: Float32 @56, TimeBetweenLookAtPlayersMax: Float32 @60, TimeBeforeFirstLookAtPlayerMin: Float32 @64, PlayerHurtThreshold: Float32 @68, TimeBetweenLookAtPlayersMin: Float32 @72 |
| VOEvent | class | TreeNodeBase | 52 | PauseTime: Float32 @24, EventType: VOEventType @28, Rows: VORow[] @32 |
| VOFacialAnimMapData | class | DataContainer | 32 | WaveAssets: WaveAsset[] @12 |
| VOGroup | class | VORowData | 52 | PauseTime: Float32 @24, Chance: Float32 @28, Lines: VOLine[] @32 |
| VOLine | class | TreeNodeBase | 64 | SourcePerson: VOPerson @24, Source: VOSource @28, Delay: Float32 @32, UrgencyLevel: Uint32 @36, CrouchAnimation: AnimationAsset @40, ScreamDialogLine: DialogLineNode @44, TalkDialogLine: DialogLineNode @48, StandAnimation: AnimationAsset @52, RadioDialogLine: DialogLineNode @56, CanSkip: Boolean @60 |
| VOListenerParam | class | VOParam | 32 | Listener: VOListenerParamType @28 |
| VOOrderParam | class | VOParam | 32 | Order: OrderReadiness @28 |
| VOParam | class | VORowData | 28 | ParameterType: VOParamType @24 |
| VOParamGroup | class | VORowData | 48 | Parameters: VOParam[] @24, GroupType: VOParamGroupType @44 |
| VOPerson | class | DataContainer | 24 | Name: String @12 |
| VOPersonParam | class | VOParam | 32 | Person: VOPerson @28 |
| VOReadinessParam | class | VOParam | 32 | Readiness: VOReadiness @28 |
| VORow | class | TreeNodeBase | 44 | RowData: VORowData[] @24 |
| VORowData | class | TreeNodeBase | 24 | - |
| VOTargetDistanceParam | class | VOParam | 32 | MaxDistance: Float32 @28 |
| VOTeamParam | class | VOParam | 32 | Team: VOTeamParamType @28 |
| VOTypesAsset | class | Asset | 84 | Vehicles: VOVehicle[] @24, Weapons: VOWeapon[] @44, Persons: VOPerson[] @64 |
| VOVehicle | class | DataContainer | 24 | Name: String @12 |
| VOVehicleParam | class | VOParam | 32 | Vehicle: VOVehicle @28 |
| WaypointAnimationData | class | DataContainer | 56 | AnimTrees: AnimTreeData[] @12, WeightParamName: String @32, LoopParamName: String @44 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27]

### RTTI-GSR-6: The HUD, menus and screen effects

Status: **verified** · Variants: PC

The UI's data in this module (the `UI` module's own types are RTTI-UI-1): HUD data, menu trees, credits, loading
screens, subtitles, screen effects (vignette, blur, colour transform).

<!-- typeinfo:gsr-ui -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| DebugConstantData | class | DataContainer | 40 | HoldGroundErrorDelayTime: Float32 @12, MinDecisionTime: Float32 @16, GraphDataBuffer: Uint32 @20, BotDebugDistance: Float32 @24, PathdataViewDistance: Float32 @28, InViewModeFOV: Float32 @32, InaccuracyEnabled: Boolean @36 |
| EntryInputActionMapDataVersion | class | DataContainer | 20 | EntryInputActionMapData: EntryInputActionMapData @12, MapDataVersion: Int32 @16 |
| GameTipAsset | class | Asset | 40 | Tips: GameTipData[] @24 |
| HudConfigAsset | class | Asset | 236 | ArtilleryAiming: WaveAsset @24, ArtilleryAimingOOA: WaveAsset @28, MortarStrikeLocking: WaveAsset @32, MPDefeatSoundMec: WaveAsset @36, MPDefeatSoundUs: WaveAsset @40, MPDefeatSoundRu: WaveAsset @44, Collectibles: CollectibleWeaponData @48, HealthVignetteStates: HealthVignetteState[] @68, SPMissionPresentationConfig: UISPMissionPresentationConfig @84, MortarStrikeFiring: WaveAsset @136, LaserDesignatorLocked: WaveAsset @140, LaserDesignatorLocking: WaveAsset @144, Vignettes: VignetteData[] @148, AmmoPickupSound: WaveAsset @164, WeaponPickupSound: WaveAsset @168, OutOfAreaVO: DialogLineNode @172, AntiSpam: AntiSpamConfig @176, Objective: GameModeObjective @184, MPVictorySoundRu: WaveAsset @208, MPVictorySoundMec: WaveAsset @212, MPVictorySoundUs: WaveAsset @216, NewObjectiveSound: WaveAsset @220, ObjectiveCompletedSound: WaveAsset @224, ObjectiveUpdatedSound: WaveAsset @228, ShowSpawnPointsOnMinimap: Boolean @232 |
| LevelSubtitles | class | DataContainer | 32 | SubtitleMaps: SubtitleLevelMap[] @12 |
| MenuConfigAsset | class | Asset | 372 | PreGameOverMovie: UIMovieAsset @24, GameOverMovies: UIMovieAsset[] @28, MenuTransitionConfig: UIMenuTransitionConfig @48, IntroMovies: UIMovieAsset[] @84, AwardGrid: UIAwardGridData @104, VehicleStatsData: UIVehicleStatsData @136, BackgroundMusic: WaveAsset @168, Eulas: EulaTexture[] @172, ExtraMovies: ExtraMoviesConfig @188, Credits: CreditsData @284, AttractionIdleTime: Float32 @324, IntroMovieMultiplayer: IntroMovieData @328, IntroMovie: IntroMovieData @348, AttractionMovie: UIMovieAsset @368 |
| MissionObjectiveHudData | class | DataContainer | 64 | HorizontalBoundaries: Vec2 @12, VerticalBoundaries: Vec2 @20, CameraPosition: Vec3 @32, CameraDistance: Float32 @48, CameraLookDistance: Float32 @52, CameraFov: Float32 @56, CameraRotation: Float32 @60 |
| SubtitleDataNode | class | TreeNodeBase | 56 | SID1: String @24, SID2: String @36, Time: Float32 @48, DisplayTime: Float32 @52 |
| SubtitleDataTree | class | TreeBase | 44 | Subtitles: SubtitleMainDataNode[] @24 |
| SubtitleLevelMap | class | DataContainer | 36 | SubtitleWavePairs: SubtitleWaveHashPair[] @12, Language: LanguageFormat @32 |
| SubtitleMainDataNode | class | TreeNodeBase | 48 | Subtitles: SubtitleDataNode[] @24, Language: LanguageFormat @44 |
| SubtitleMapAsset | class | Asset | 44 | Timings: SubtitleTimingFile[] @24 |
| SubtitleRuntimeData | class | DataContainer | 32 | DisplayTime: Float32 @12, Time: Float32 @16, SID: String @20 |
| SubtitleTimingFile | class | DataContainer | 28 | Language: LanguageFormat @12, SourceFile: FileRef @16 |
| SubtitleWaveHashPair | class | DataContainer | 40 | Subtitles: SubtitleRuntimeData[] @12, OriginalHashedWaveName: Uint32 @32, HashedWaveName: Uint32 @36 |
| UIInputNodeDescription | class | DataContainer | 28 | DescriptionSID: String @12, Input: InputNodeType @24 |
| UISharedConfigAsset | class | Asset | 272 | CollectiblesGrid: UICollectiblesData @24, HudRenderConfig: HudRenderConfig @64, FeatureHints: UIFeatureHintData[] @112, WeaponDataVietnam: UIWeaponData[] @128, UnlocksDataVietnam: UIUnlocksData @144, UnlocksData: UIUnlocksData @176, InputSchemes: UIInputSchemeInfo[] @208, WeaponData: UIWeaponData[] @224, KillMsgSIDMapping: UIKillMsgSIDMapping[] @240, WeaponStatsData: UIWeaponStatsPresentation[] @256 |
| UISubRipAsset | class | Asset | 40 | Entries: UISubRipEntry[] @24 |
| VersionData | class | Asset | 68 | DateTime: String @24, BranchId: String @36, disclaimer: String @48, Version: Int32 @60, BuildSucceeded: Boolean @64, FinalBundles: Boolean @65 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27]

### RTTI-GSR-7: Persistence, online, settings and modes

Status: **verified** · Variants: PC

Awards, stats, scoring, unlocks, achievements, matchmaking, rich presence, the settings objects and the game modes
(`engine/persistence.md`, `network.md`, `settings.md`).

<!-- typeinfo:gsr-persistence -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AchievementsDataAsset | class | Asset | 40 | Achievements: AchievementData[] @24 |
| AnimatedSoldierRagdollSettingsData | class | DataContainer | 36 | InstantRagdollImpulseThreshold: Float32 @12, InstantRagdollSpeedThreshold: Float32 @16, Ragdoll: RagdollAsset @20, RagdollMaterial: String @24 |
| AwardData | class | TreeNodeBase | 200 | VO: DialogLineNode @24, Score: Float32 @28, Sound: WaveAsset @32, SoundName: String @36, Repeat: AwardType @48, Group: AwardGroup @52, Bucket: ScoringBucket @56, MaxRepetitions: Int32 @60, Dependencies: CriteriaAward[] @64, ChildAwards: AwardData[] @80, Description: String @100, License: String @112, Criteria: CriteriaData[] @124, Code: String @144, Image: TextureAsset @156, ImageName: String @160, UIArgument: String @172, UIName: String @184, Visible: Boolean @196 |
| AwardDataTree | class | AwardableTreeBase | 88 | Awards: AwardData[] @68 |
| AwardStarData | class | AwardData | 204 | StarTemplate: AwardStarNameData @200 |
| AwardStarNameData | class | DataContainer | 28 | NameInstances: AwardStarNameInstance[] @12 |
| AwardableTreeBase | class | TreeBase | 68 | Scales: EventScaleData[] @24, GlobalScaleFactor: Float32 @40, CategoryTrees: StatCategoriesTree[] @44, GeneralStatistics: Boolean @64 |
| BigWorldSetting | class | DataContainer | 32 | MinDelayTimeInMinutes: Float32 @12, MaxDelayTimeInMinutes: Float32 @16, MaxDistance: Int32 @20, Sound: SoundTemplate @24, MinDistance: Int32 @28 |
| BigWorldSettingsAsset | class | Editable | 44 | Settings: BigWorldSetting[] @24 |
| CriteriaData | class | DataContainer | 56 | CriteriaDescription: String @12, CriteriaType: CriteriaType @24, Scale: Float32 @28, ScaleFactor: Float32 @32, Measuring: StatEvent @36, Value: Float32 @40, ParamY: StatsCategoryData @44, ParamX: StatsCategoryData @48, ReadableByClient: Boolean @52, ReadableByServer: Boolean @53, ShouldSummarize: Boolean @54, ShouldHide: Boolean @55 |
| CriteriaStarCategoryData | class | DataContainer | 28 | CriteriaInstances: CriteriaStarCategoryInstance[] @12 |
| CriteriaStarData | class | CriteriaData | 60 | StarTemplate: CriteriaStarCategoryData @56 |
| DifficultyModesData | class | Asset | 44 | DifficultyModes: DifficultyData[] @24 |
| DogTagData | class | DataContainer | 60 | SoundName: String @12, VO: DialogLineNode @24, DogTagType: DogTagType @28, UIName: String @32, Rank: Uint32 @44, ImageName: String @48 |
| EntitlementAsset | class | DataContainer | 32 | Queries: EntitlementQuery[] @12 |
| EntitlementQuery | class | DataContainer | 92 | ProductId: String @12, GroupName: String @24, ProductCatalog: String @36, EntitlementTag: String @48, Entitlements: Entitlement[] @60, EntitlementId: String @76, CountOnly: Boolean @88, IsHighPriority: Boolean @89, Global: Boolean @90 |
| GameModeData | class | Asset | 156 | CrosshairsName: String @24, InGameMenuName: String @36, LoaderName: String @48, AddToIngameBundleRefs: FileRef @60, AddToLoaderBundleRefs: FileRef @72, SecondaryLogicLayerName: String @84, MatchmakingQueryID: String @96, ModeID: String @108, Description: String @120, PrimaryLogicLayerName: String @132, HudRootName: String @144 |
| GameSettingsAsset | class | Asset | 40 | Categories: GameSettingsCategory[] @24 |
| IndoorDampingSetting | class | DataContainer | 20 | Damping: Float32 @12, MaxArea: Float32 @16 |
| MatchMakingFilter | class | MatchMakingRule | 84 | DefaultValues: String[] @52, PossibleValues: String[] @68 |
| MatchMakingIntegerPreference | class | MatchMakingRule | 84 | MatchingOption: MatchMakingMatchingOption @52, FitThresholds: MatchMakingFitThreshold[] @56, DefaultValue: Int32 @72, Weight: Uint32 @76, Scale: Uint32 @80 |
| MatchMakingMap | class | DataContainer | 68 | Licenses: String @12, Revision: Uint32 @24, GameMode: String @28, UIName: String @40, MapName: String @52, KickIfLicenseMissing: Boolean @64 |
| MatchMakingPlaylist | class | DataContainer | 52 | RoundsPerMap: Uint32 @12, Maps: MatchMakingMapLink[] @16, Name: String @32, Rule: MatchMakingPlaylistRule @44, IsDefault: Boolean @48 |
| MatchMakingRule | class | DataContainer | 52 | HideOnRuleVisible: String @12, Match: String @24, Rule: String @36, SkipIfDefault: Boolean @48 |
| MatchMakingSession | class | DataContainer | 124 | HostSetupTimeout: Uint32 @12, PoolPlayerCountThresholds: MatchMakingPoolPlayerCountThreshold[] @16, ListServersMaxCount: Uint32 @32, PoolPlayerTimeout: Uint32 @36, PoolMaxPlayers: Uint32 @40, Filters: MatchMakingFilter[] @44, SessionType: MatchMakingSessionType @64, SessionIdentifier: String @68, StringPreferences: MatchMakingStringPreference[] @80, IntegerPreferences: MatchMakingIntegerPreference[] @100, CountServersAvailableForReset: Boolean @120 |
| MatchMakingStringPreference | class | MatchMakingRule | 132 | VotingMethod: MatchMakingVotingMethod @52, MatchingOption: MatchMakingMatchingOption @56, Variant: MatchMakingPreferenceVariant @60, FitThresholds: MatchMakingFitThreshold[] @64, Labels: String[] @80, Weight: Uint32 @96, DefaultValues: String[] @100, Fit: Float32[] @116 |
| MaterialSoldierSoundSettings | class | DataContainer | 24 | LandSound: SoundAsset @12, ProneSound: SoundAsset @16, FootStepSound: SoundAsset @20 |
| MiddlegrowthModelGroupMemberData | class | GroupMemberData | 68 | SoundCollisionScale: Float32 @60, MeshEntityType: TreeMeshEntityData @64 |
| OnlineProviderAsset | class | Asset | 40 | Configurations: OnlineProviderConfiguration[] @24 |
| PersistenceData | class | Asset | 120 | Unlocks: PersistenceWeaponData[] @24, UnlockTokens: UnlockTokenData @44, Entitlements: EntitlementAsset @48, SPAchievements: AchievementsDataAsset @52, Partitions: PersistentPartition[] @56, Final: PersistentValueTemplateData[] @76, Remapping: PersistentValueNameRemapping[] @96, Collectibles: Boolean @116, Consumables: Boolean @117 |
| PersistenceGameData | class | Asset | 44 | Assets: Asset[] @24 |
| PersistentDerivedValueTemplateData | class | PersistentValueTemplateData | 72 | Formula: String @60 |
| PersistentPartition | class | DataContainer | 36 | Title: String @12, Domain: String @24 |
| PersistentSpecialization | class | PersistentWeapon | 100 | KitCategory: KitCategory @84, WeaponClass: WeaponClassEnum @88, SpecializationType: PersistentSpecializationType @92, Modifier: Float32 @96 |
| PersistentValueNameRemapping | class | DataContainer | 36 | External: String @12, Internal: String @24 |
| PersistentValueTemplateData | class | DataContainer | 60 | ClientFlags: PersistentValueProtection @12, ServerFlags: PersistentValueProtection @16, WritePolicy: PersistentValueWritePolicy @20, ValueType: PersistentValueType @24, Name: String @28, Description: String @40, DefaultValue: Float32 @52, Authorative: Boolean @56 |
| RichPresenceConnection | class | DataContainer | 48 | PropertyPairs: RichPresencePropertyPair[] @12, ContextPairs: RichPresenceContextPair[] @28, RichPresenceIndex: Uint32 @44 |
| RichPresenceContext | class | DataContainer | 40 | ContextType: RichPresenceContextType @12, PossibleValues: RichPresenceContextValue[] @16, Index: Uint32 @36 |
| RichPresenceContextValue | class | DataContainer | 16 | Index: Uint32 @12 |
| RichPresenceData | class | Asset | 92 | Contexts: RichPresenceContext[] @24, Properties: RichPresenceContext[] @44, Inactive: RichPresenceString @64, Strings: RichPresenceString[] @68, Default: RichPresenceString @88 |
| RichPresenceProperty | class | DataContainer | 20 | PropertyType: RichPresencePropertyType @12, Index: Uint32 @16 |
| RichPresenceString | class | DataContainer | 16 | Index: Uint32 @12 |
| RigidModelGroupMemberData | class | GroupMemberData | 64 | MeshEntityType: RigidMeshEntityData @60 |
| SoldierRagdollSettingsData | class | DataContainer | 128 | StaticBulletImpulse: Vec3 @16, StaticCollisionImpulse: Vec3 @32, StaticDeathImpulse: Vec3 @48, StaticExplosionImpulse: Vec3 @64, BulletImpulseMultiplier: Float32 @80, MaxImpulse: Float32 @84, HitBoneImpulseModifier: Float32 @88, CollisionImpulseMultiplier: Float32 @92, DeathSpasms: RagdollImpulse[] @96, DeathImpulseMultiplier: Float32 @112, ExplosionImpulseMultiplier: Float32 @116 |
| SoldierSprintSettingsData | class | DataContainer | 56 | ThrottleInputRequired: Float32 @12, FovOutTime: Float32 @16, InterruptingActions: EntryInputActionEnum[] @20, RecoverTime: Float32 @36, FovInDelay: Float32 @40, Fov: Float32 @44, FovOutDelay: Float32 @48, FovInTime: Float32 @52 |
| StaticModelGroupMemberData | class | GroupMemberData | 108 | NetworkIdCountPerInstance: Uint32 @60, FirstPartHealthStateNetworkIds: Uint32[] @64, FirstPartHealthStateIndices: Uint32[] @80, MeshEntityType: EntityData @96, FirstNetworkId: Uint32 @100, NetworkIdCount: Uint32 @104 |
| StatsCategoryData | class | TreeNodeBase | 104 | Code: String @24, SID: String @36, CheckedMembers: StatsCategoryReferencedData[] @48, Categories: StatsCategoryData[] @68, Members: String[] @88 |
| StatsCategoryReferencedData | class | DataContainer | 12 | - |
| StatsCategoryReferencedObjectData | class | StatsCategoryReferencedData | 44 | ObjectInstanceGuid: Guid @12, ObjectClassName: String @28, Object: GameObjectData @40 |
| StatsCategoryReferencedSoldierKitData | class | StatsCategoryReferencedData | 16 | KitCategory: KitCategory @12 |
| StatsCategoryReferencedSoldierRoleData | class | StatsCategoryReferencedData | 16 | Role: SoldierRole @12 |
| UnlockToken | class | DataContainer | 20 | Tokens: Uint32 @12, Score: Float32 @16 |
| UnlockTokenData | class | Asset | 44 | Tokens: UnlockToken[] @24 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27]

### RTTI-GSR-8: The rest of the module

Status: **verified** · Variants: PC

Everything else of the module that is not an entity: assets (teams, levels, materials, kits), cameras, the soldier's
and the vehicles' data parts, input actions, physics helpers, sound maps.

<!-- typeinfo:gsr-other -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AlternateCameraViewData | class | DataContainer | 160 | MeshOffset: Vec3 @16, FadeInDuration: Float32 @32, FadeOutDuration: Float32 @36, InputSuppression: InputSuppressionData @40, BlackDuration: Float32 @56, FieldOfView: Float32 @60, FovTransitionTime: Float32 @64, Mesh: RigidMeshAsset @68, Hud: HudData @72, ToggleViewChange: Boolean @152, FadeToBlack: Boolean @153 |
| AmbientSoundsAsset | class | Editable | 120 | MaterialBigWorld: MaterialBigWorldMapData[] @24, SoundMaxHeight: Float32 @40, IndoorDampings: IndoorDampingSetting[] @44, GlobalAmbientSounds: SoundAsset[] @64, BigWorld: BigWorldSettingsAsset[] @84, MaterialSounds: MaterialSoundMapData[] @104 |
| AnimatedCameraData | class | CameraData | 96 | FovBone: String @64, CameraBone: String @76, Skeleton: SkeletonAsset @88 |
| AnimationData | class | TreeNodeBase | 128 | Movement: Vec3 @32, EndState: AnimationStateData @48, StartState: AnimationStateData @64, Readiness: ReadinessLevel @80, Animation: AnimationAsset @84, Fire: FireState @88, ObstacleDistance: Float32 @92, ObstacleHeight: Float32 @96, EndAngle: Float32 @100, Index: Int32 @104, Duration: Float32 @108, Enabled: Boolean @112 |
| AnimationMovementAsset | class | TreeBase | 44 | Animations: AnimationData[] @24 |
| AnimationSoundMapData | class | Asset | 40 | Categories: AnimationSoundCategoryData[] @24 |
| AxesInputActionData | class | InputActionData | 28 | ScaleValue: Float32 @16, Axis: InputDeviceAxes @20, NormalizeInput: Boolean @24 |
| BangerSpawnImpulseParams | class | DataContainer | 40 | MinStrength: Float32 @12, MaxStrength: Float32 @16, WaterStrengthModifier: Float32 @20, MaxVerticalAngle: Float32 @24, MinHorizontalAngle: Float32 @28, MaxHorizontalAngle: Float32 @32, MinVerticalAngle: Float32 @36 |
| BoneFakePhysicsData | class | DataContainer | 32 | BoneId: Int32 @12, BoneName: String @16, FakePhysics: FakePhysicsData @28 |
| BoundingBoxAsset | class | Asset | 40 | Boxes: OrientedBoundingBox[] @24 |
| BoundingBoxAssetMetaData | class | DataContainer | 28 | Boxes: OrientedBoundingBox[] @12 |
| BoundingBoxExportResult | class | DataContainer | 24 | OutputFileName: String @12 |
| CameraData | class | DataContainer | 64 | OcclusionRayOffset: Vec3 @16, SoundOcclusion: Float32 @32, HudFx: HudFxData @36, ShadowViewDistance: Float32 @40, FadeTime: Float32 @44, PreFadeTime: Float32 @48, FadeWaitTime: Float32 @52, StayFadedWhileStreaming: Boolean @56, FadeDuringJump: Boolean @57 |
| CarControlConstantData | class | DataContainer | 16 | YawStrength: Float32 @12 |
| ChaseCameraData | class | TargetCameraData | 304 | ToWantedPositionScale: Vec3 @176, TargetOffset: Vec3 @192, PillMaximumCollisionRadius: Float32 @208, PillMinimumCollisionLength: Float32 @212, PillMinimumCollisionRadius: Float32 @216, MaxVelocity: Float32 @220, VelocityDrag: Float32 @224, UpdateRate: Uint32 @228, SoundShakeFactor: Float32 @232, LookDistanceInFrontOfTarget: Float32 @236, PillExpandSizeSpeedAcceleration: Float32 @240, LookDistanceScale: Float32 @244, AwayFromTargetForceScale: Float32 @248, WantedAngleDeg: Float32 @252, WantedDistance: Float32 @256, TargetRotationOffset: Float32 @260, MaxViewRotationAngleDeg: Float32 @264, MaxDistance: Float32 @268, CollisionRadius: Float32 @272, ForceFieldForceScale: Float32 @276, SnapDistance: Float32 @280, ForceFieldRadius: Float32 @284, HasCollision: Boolean @288, KeepTargetPitch: Boolean @289, ShouldRollWithTarget: Boolean @290, InheritTargetVelocity: Boolean @291 |
| ChildMovingBodyData | class | LinearMovingBodyData | 128 | InputAction: EntryInputActionEnum @96, TriggerImpulse: Float32 @100, InputDelay: Float32 @104, Speed: Float32 @108, IsOneShotInput: Boolean @112 |
| ChildRotationBodyData | class | RotationBodyData | 76 | WorldSpaceLockEfficiency: Float32 @52, RotationInput: EntryInputActionEnum @56, ResetForceInputThreshold: Float32 @60, ForceModifier: Float32 @64, ResetForceModifier: Float32 @68, UseLinearInput: Boolean @72, WorldSpaceLock: Boolean @73 |
| CombatConstantData | class | DataContainer | 124 | InvestigateFreeLookRadius: Float32 @12, InvestigatePauseTimeMin: Float32 @16, InvestigatePauseTimeMax: Float32 @20, InvestigateAroundTargetRadius: Float32 @24, SuppressMinLOFOffsetDistance: Float32 @28, InvestigateObeyFrontLineDistance: Float32 @32, InvestigateCoverSearchWidth: Float32 @36, StationaryBailDistance: Float32 @40, PrecisionTimeout: Float32 @44, CloseRangeAccuracyDistance: Float32 @48, StandStillMinTime: Float32 @52, MoveInCombatDelay: Float32 @56, NewEnemyDecideMaxDistance: Float32 @60, AvoidC4Time: Float32 @64, ExplosiveObjectSearchRadius: Float32 @68, ExplosiveKnockdownDistance: Float32 @72, ExplosiveBlastMultiplier: Float32 @76, PoseChangeLockoutTime: Float32 @80, PoseTransitionFireDelay: Float32 @84, ChangePoseStopDelay: Float32 @88, AllowCrouchFiringWaterLevel: Float32 @92, MaxSuppressDistance: Float32 @96, MaxSuppressorsRatio: Float32 @100, SuppressMinLOFDistance: Float32 @104, MaxSuppressPositionTime: Float32 @108, MeleeTargetAngle: Float32 @112, MoveOutOfCoverTargetAngle: Float32 @116, MoveOutOfCoverLineOfSightTest: Boolean @120 |
| CutsceneCameraData | class | AnimatedCameraData | 144 | AnimTree: AnimTreeData @96, TotalLength: Float32 @100, CameraLeaps: CameraLeapData[] @104, Animation: AnimationAsset @120, DofBone: String @124, UseDof: Boolean @136 |
| CutsceneEntityCameraData | class | CutsceneCameraData | 160 | PeekAroundMaxPitch: Float32 @144, PeekAroundMaxYaw: Float32 @148, PeekAroundSnapBack: Boolean @152 |
| CutsceneEntityScreenFadeData | class | DataContainer | 40 | PostCutsceneFadeOutTime: Float32 @12, PostCutsceneMinBlackDelay: Float32 @16, PostCutsceneFadeInTime: Float32 @20, StartPostFadeOutBeforeEndTime: Float32 @24, PreCutsceneFadeOutTime: Float32 @28, PreCutsceneMinBlackDelay: Float32 @32, PreCutsceneFadeInTime: Float32 @36 |
| DifficultyData | class | DataContainer | 128 | SnapBoxModifier: Vec3 @16, StickyBoxModifier: Vec3 @32, HumanInCriticalHealthDamageModifier: Float32 @48, HumanInCriticalHealth: Float32 @52, VehicleDamageModifier: Float32 @56, CriticalHealthJesusModeTimeModifier: Float32 @60, SuckZoomModifier: Float32 @64, HelicopterHoverTimeMultiplier: Float32 @68, ReadinessReactionTimeMultiplier: Float32 @72, FriendsHealthModifier: Float32 @76, EnemiesHealthModifier: Float32 @80, Name: String @84, HumanHealthModifier: Float32 @96, AccuracyIncreaseTimeModifier: Float32 @100, FinalAccuracyModifierStanding: Float32 @104, FinalAccuracyModifierCrouching: Float32 @108, StartAccuracyModifierStanding: Float32 @112, StartAccuracyModifierCrouching: Float32 @116, UsePitchZoomSnap: Boolean @120 |
| EditableAction | class | DataContainer | 16 | Concept: InputConceptIdentifiers @12 |
| EntryInputActionMapData | class | InputActionMapData | 40 | - |
| EntryInputActionMappingData | class | InputActionMappingData | 20 | ConceptIdentifier: InputConceptIdentifiers @12, ActionIdentifier: EntryInputActionEnum @16 |
| EntryInputActionMapsData | class | Asset | 52 | InputActionMaps: EntryInputActionMapData[] @24, DefaultInputActionMap: Int32 @44, ActionMapSettingsScheme: Int32 @48 |
| EnvironmentConstantData | class | DataContainer | 16 | BangerIncludeDistance: Float32 @12 |
| ExplosiveObjectData | class | DataContainer | 128 | BoxMax: Vec3 @16, BoxMin: Vec3 @32, Transform: LinearTransform @48, ShockwaveRadius: Float32 @112, ExplosionRadius: Float32 @116, Enumeration: Uint32 @120 |
| FPSCameraData | class | TargetCameraData | 464 | BlurEffectSettings: BlurEffectData @176, SoundShakeFactor: Float32 @200, TurnEffectSettings: TurnEffectData @204, ImpulseSettings: CameraImpulseShakeData @236, BulletImpulseSettings: CameraImpulseShakeData @344 |
| FakeAnimatedSoldierCollisionData | class | Asset | 68 | SaveLatencyInfoTime: Float32 @24, TimeEnableAimAfterShooting: Float32 @28, SaveLatencyInfoCount: Int32 @32, FakePoses: FakeAnimatedSoldierCollisionPose[] @36, FakeBoneInfo: FakePoseLimbInfo[] @52 |
| FileQueryData | class | Asset | 40 | Results: String[] @24 |
| FirstPersonCameraData | class | DataContainer | 144 | Rotation: Vec3 @16, Offset: Vec3 @32, ReleaseModifierYaw: Float32 @48, ReleaseModifierRoll: Float32 @52, ReleaseModifierPitch: Float32 @56, ZoomRotateRollModifier: Float32 @60, ReleaseModifier: Float32 @64, OffsetReleaseModifier: Float32 @68, ZoomOffsetReleaseModifier: Float32 @72, RotationAdditionToOffset: Float32 @76, ZoomReleaseModifierRoll: Float32 @80, ZoomReleaseModifierPitch: Float32 @84, ZoomReleaseModifierYaw: Float32 @88, MoveForwardModifier: Float32 @92, RotateYawModifier: Float32 @96, MoveStrafeModifier: Float32 @100, WeaponSpringEffect: WeaponLagSpringEffectData @104, CameraSpringEffect: WeaponLagSpringEffectData @108, RotatePitchModifier: Float32 @112, ZoomRotateYawModifier: Float32 @116, ZoomRotatePitchModifier: Float32 @120, ZoomMoveForwardModifier: Float32 @124, RotateRollModifier: Float32 @128, ZoomMoveStrafeModifier: Float32 @132 |
| FollowConstantData | class | DataContainer | 56 | SlowStartDistance: Float32 @12, FollowInertia: Float32 @16, SlowStartMinTime: Float32 @20, LookAwayAngle: Float32 @24, SlowStopDistance: Float32 @28, FollowMimicPoseDistance: Float32 @32, FollowHumanMaxDistance: Float32 @36, DistanceFromPlayerBeforeTeleport: Float32 @40, FollowHumanInCombatMaxDistance: Float32 @44, FollowAiInCombatMaxDistance: Float32 @48, FollowAiMaxDistance: Float32 @52 |
| GroupMemberData | class | DataContainer | 60 | PhysicsPartCount: Uint32 @12, PhysicsPartCountPerInstance: Uint32 @16, InstanceCount: Uint32 @20, FirstPhysicsPart: Uint32 @24, SpawnDatas: GroupMemberInstanceSpawnData[] @28, MemberType: GamePhysicsEntityData @44, Name: String @48 |
| HaloAimerData | class | DataContainer | 192 | StickyDistanceScale: Vec3 @16, StickyBoxScale: Vec3 @32, SnapDistanceScale: Vec3 @48, SnapBoxScale: Vec3 @64, SuckStartInputThreshold: Float32 @80, SuckOwnRequiredMovementForMaximumSuck: Float32 @84, SuckTargetSpeedInfluence: Float32 @88, SuckZoomingMultiplier: Float32 @92, SuckPitch: Float32 @96, SuckYaw: Float32 @100, SuckZoomingPostTime: Float32 @104, AccelerationInputThreshold: Float32 @108, AccelerationMultiplier: Float32 @112, SuckDistanceFallOff: Float32[] @116, InputPolynomial: Float32[] @132, ZoomedInputPolynomial: Float32[] @148, AccelerationDamping: Float32 @164, SuckUserInputMultiplier: Float32 @168, SuckOwnSpeedInfluence: Float32 @172, SuckSoftZone: Float32 @176, YawSpeed: Float32 @180, PitchSpeed: Float32 @184, UsePitchAcceleration: Boolean @188, UseYawAcceleration: Boolean @189 |
| HealthStateData | class | DataContainer | 92 | SpawnedBangerImpulseParams: BangerSpawnImpulseParams @12, LoosePartPhysics: NetworkableLoosePartPhysicsData @16, PreferedShapeType: ShapeType @20, MeshName: String @24, Health: Float32 @36, Explosion: ExplosionEntityData @40, Effects: EffectAsset[] @44, SpawnedBanger: DynamicPhysicsEntityData @64, LoosePartPhysicsArray: NetworkableLoosePartPhysicsData[] @68, CopyDamageToBanger: Boolean @88 |
| HelicopterControlConstantData | class | DataContainer | 212 | MinAttackDistance: Float32 @12, MaxAttackHeightDiff: Float32 @16, AttackSideSearchDistance: Float32 @20, CombatMovementRadius: Float32 @24, AttackHeight: Float32 @28, PostAttackClearDistance: Float32 @32, YawAngularSpeedDamper: Float32 @36, YawDifferenceToAngularSpeedScale: Float32 @40, MaxRollInput: Float32 @44, AttackDistance: Float32 @48, MaxYawInput: Float32 @52, YawAngularSpeedToYawInputScale: Float32 @56, TargetProximity: Float32 @60, EvasiveEstimatedImpactTime: Float32 @64, EvasiveMiniumSpeed: Float32 @68, TurnGunnerCheckAngle: Float32 @72, EvasiveWaypointRadius: Float32 @76, EvasiveAltitude: Float32 @80, EvasiveDeflectionAngle: Float32 @84, AttackPilotHeight: Float32 @88, AttackHoverTime: Float32 @92, TargetTooClose: Float32 @96, PilotMoveStep: Float32 @100, AttackPilotMaxSpeed: Float32 @104, PilotHeightSpan: Float32 @108, AlwaysAttainHeightDistance: Float32 @112, MaxBrake: Float32 @116, MaxThrottle: Float32 @120, SidewaysSpeedDamper: Float32 @124, ForwardsSpeedDamper: Float32 @128, MovementToVelocityScale: Float32 @132, HeightSpeedDamper: Float32 @136, MinSpeedForHeightSpeedRestriction: Float32 @140, HeightToSpeedScale: Float32 @144, HeightTiltInfluence: Float32 @148, HeightSpeedToBrakeScale: Float32 @152, HeightSpeedToThrottleScale: Float32 @156, ForwardsSpeedToPitchScale: Float32 @160, MaxRoll: Float32 @164, PitchAimOverride: Float32 @168, MaxPitchInput: Float32 @172, RollAngularSpeedToRollInputScale: Float32 @176, RollAngularSpeedDamper: Float32 @180, RollToAngularSpeedScale: Float32 @184, MaxAimPitch: Float32 @188, MaxMovementPitch: Float32 @192, SidewaysSpeedToRollScale: Float32 @196, PitchAngularSpeedToPitchInputScale: Float32 @200, PitchAngularSpeedDamper: Float32 @204, PitchToAngularSpeedScale: Float32 @208 |
| HotItemAsset | class | ObjectBlueprint | 84 | - |
| IndexedAnimationData | class | DataContainer | 68 | StopParameter: String @12, WeightParameter: String @24, StartParameter: String @36, AnimTree: AnimTreeDataAsset @48, IndexParameter: String @52, AddOnDemand: Boolean @64 |
| InputActionData | class | DataContainer | 16 | NegateValue: Boolean @12, IsAnalog: Boolean @13 |
| InputActionMapData | class | DataContainer | 40 | Slot: InputActionMapSlot @12, PlatformSpecific: InputActionMapPlatform @16, Actions: InputActionsData[] @20 |
| InputActionMappingData | class | DataContainer | 12 | - |
| InputActionMappingsData | class | DataContainer | 32 | Mappings: InputActionMappingData[] @12 |
| InputActionsData | class | DataContainer | 36 | InputActions: InputActionData[] @12, ConceptIdentifier: InputConceptIdentifiers @32 |
| InputConfigurationAsset | class | Asset | 64 | CrouchAction: EntryInputActionEnum @24, CrouchAndHoldAction: EntryInputActionEnum @28, SprintReleaseTime: Float32 @32, DefaultInputConceptDefinition: EntryInputActionMapsData @36, UserConfigurableActionMaps: EditableActionMap[] @40, TogglePoseAction: EntryInputActionEnum @56, DisableCrawlingWhileReloading: Boolean @60, ReleaseSprintWhenReleasingForward: Boolean @61, ProneToStandOnJump: Boolean @62, DisableCrawlingWhileFiring: Boolean @63 |
| KeyboardInputActionData | class | InputActionData | 20 | Key: InputDeviceKeys @16 |
| LevelData | class | WorldData | 960 | EmitterSystemMetrics: EmitterSystemMetrics @124, LevelDescription: LevelDescription @132, InfantryFOVMultiplier: Float32 @496, DefaultFOV: Float32 @500, SpawnPointManager: SpawnPointManagerData @504, ExclusiveStaticCullTree: StaticCullTreeData @508, MaxLocalPathfindingObjects: Uint32 @512, ClientTreeStaticCullTree: StaticCullTreeData @516, ClientStaticCullTree: StaticCullTreeData @520, FriendlyDamageModifier: Float32 @524, HardcorePlayerHealthModifier: Float32 @528, BloodEffect: EffectAsset @532, HardcoreRegenerateHealthModifier: Float32 @536, HardcoreFriendlyDamageModifier: Float32 @540, MaxVehicleHeight: Float32 @544, AmbientSounds: AmbientSoundsAsset @548, MinimapSettings: MinimapSettings @560, SoundStates: SoundStateSettingsAsset @752, ReverbSettings: ReverbSettingsAsset @756, PathObjectCount: Uint32 @760, VoiceOverLines: DialogLineNode[] @764, SubtitleAsset: SubtitleMapAsset @784, TriggerAnimations: AnimationAsset[] @788, VoiceOverAnimTree: AnimTreeData @808, VoiceOvers: VOAsset[] @812, Covers: CoverData[] @832, DynamicObjects: GameObjectData[] @852, WaypointAnimations: WaypointAnimationData @872, PathfindingTypes: PathfindingSettings[] @876, AerialHeightmapCellSize: Float32 @896, AerialHeightmapQuality: Int32 @900, DestructibleLocalPathfindingParts: DestructibleLocalPathfindingPart[] @904, ExplosiveObjects: ExplosiveObjectData[] @920, UseBC2SP09ShadowHack: Boolean @940, HasFlyingAI: Boolean @941, HasAI: Boolean @942, StartLevelWithSoundFaded: Boolean @943, StartLevelFadedToBlack: Boolean @944 |
| LevelDescriptionAsset | class | Asset | 400 | Description: LevelDescription @24, LevelName: String @388 |
| LevelSaveData | class | DataContainer | 32 | SaveNames: String[] @12, SaveSize: Uint32 @28 |
| LookConstantData | class | DataContainer | 56 | LookAroundClampAngle: Float32 @12, EyeTargeterSpringModifier: Float32 @16, LookAroundMinRandomWaitTime: Float32 @20, LookAroundRandomAfterObjectModifier: Float32 @24, LookAroundMaxRandomWaitTime: Float32 @28, EyeTargeterMaxYawOffset: Float32 @32, EyeTargeterMaxOffsetTime2: Float32 @36, EyeTargeterMinOffsetTime2: Float32 @40, EyeTargeterMinPitchOffset: Float32 @44, EyeTargeterMinYawOffset: Float32 @48, EyeTargeterMaxPitchOffset: Float32 @52 |
| MatchMakerAsset | class | Asset | 180 | Playlists: MatchMakingPlaylist[] @24, PoolOverride: MatchMakingPoolOverride[] @44, Maps: MatchMakingMap[] @60, EloParams: EloParameters @80, Sessions: MatchMakingSession[] @160 |
| MaterialMapData | class | DataContainer | 32 | Size: Uint32 @12, Data: Uint8[] @16 |
| MaterialPropertyAIData | class | MaterialRelationPropertyData | 20 | SoftCoverHeight: Float32 @16 |
| MaterialPropertyEffectData | class | MaterialRelationPropertyData | 20 | ImpactEffect: EffectAsset @16 |
| MaterialPropertyHealthData | class | MaterialRelationPropertyData | 20 | Health: Float32 @16 |
| MaterialPropertyImpulseData | class | MaterialRelationPropertyData | 20 | ImpulseAbsorptionMultiplier: Float32 @16 |
| MaterialPropertySootData | class | MaterialRelationPropertyData | 20 | Soot: SootMaterialData @16 |
| MaterialPropertySoundData | class | MaterialRelationPropertyData | 36 | SoldierSettings: MaterialSoldierSoundSettings @16, Softness: Float32 @20, ScrapeLength: Float32 @24, ImpactSound: SoundAsset @28, ScrapeSound: SoundAsset @32 |
| MaterialPropertyTerrainData | class | MaterialRelationPropertyData | 48 | DirtTriggerColor: Vec3 @16, DirtTriggerFactor: Float32 @32, DestructionEffect: EffectAsset @36 |
| MaterialRelationEffectData | class | MaterialRelationPropertyData | 20 | ImpactEffect: EffectAsset @16 |
| MaterialRelationSoundData | class | MaterialRelationPropertyData | 28 | ScrapeLength: Float32 @16, ScrapeSound: SoundAsset @20, ImpactSound: SoundAsset @24 |
| MaterialRelationTerrainDestructionData | class | MaterialRelationPropertyData | 24 | Depth: Float32 @16, Width: Float32 @20 |
| MaterialRelationVehicleData | class | MaterialRelationPropertyData | 32 | WheelEffect: EffectAsset @16, GroundEffect: EffectAsset @20, ChassiEffect: EffectAsset @24, TrackEffect: EffectAsset @28 |
| MobilityData | class | DataContainer | 20 | MaxSpeed: Float32 @12, Use3DWaypoints: Boolean @16 |
| MouseInputActionData | class | AxesInputActionData | 36 | Button: InputDeviceMouseButtons @28, RememberExcessInput: Boolean @32, SimulateJoystickAxis: Boolean @33 |
| NetworkableLoosePartPhysicsData | class | LoosePartPhysicsData | 20 | Networked: Boolean @16 |
| ObjectData | class | DataContainer | 40 | StrengthType: StrengthType @12, ReuseTime: Float32 @16, Mobility: MobilityData @20, EquipmentType: AIVehicleBehaviourData @24, Armament: ArmamentData @28, InterludeOnly: Boolean @32, InvalidForAI: Boolean @33, HasExposedSoldier: Boolean @34, Forbidden: Boolean @35, ForbiddenForHuman: Boolean @36 |
| OneOptionAxisEditableAction | class | EditableAction | 32 | Header: String @16, Invert: Boolean @28 |
| OneOptionButtonEditableAction | class | EditableAction | 28 | Header: String @16 |
| PadInputActionData | class | AxesInputActionData | 40 | Pov: InputDevicePOVs @28, Button: InputDevicePadButtons @32, UseSquareInput: Boolean @36 |
| PartLinkData | class | DataContainer | 28 | PartComponent1: GameObjectData @12, PartComponent2: GameObjectData @16, PartComponentIndex1: Uint32 @20, PartComponentIndex2: Uint32 @24 |
| PointSystemParamsAsset | class | Asset | 1444 | SaviorDamageLimit: Float32 @24, KillAssistLimit: Float32 @28, CriticalKillAssistLimit: Float32 @32, DamageTeamVehicleLimit: Float32 @36, TeamHealingLimit: Float32 @40, TeamDamageLimit: Float32 @44, DefensiveKillBonusRadius: Float32 @48, OffensiveKillBonusRadius: Float32 @52, LongRangeHeadshotThreshold: Float32 @56, ObjectiveDamageLimit: Float32 @60, VehicleKillAssistLimit: Float32 @64, VehicleDestroyAssistLimit: Float32 @68, DamageVehicleLimit: Float32 @72, VehicleKillAssistScore: ScoreEntry @76, VehicleDestroyAssistScore: ScoreEntry @96, SquadVehicleDestroyAssistScore: ScoreEntry @116, ObjectiveDestructionScore: ScoreEntry @136, ObjectiveTeamDestructionScore: ScoreEntry @156, DefensiveKillBonusScore: ScoreEntry @176, AvengerKillTimeout: Float32 @196, SaviorKillTimeout: Float32 @200, RepairTeamVehicleLimit: Float32 @204, MotionSensorAssistScore: ScoreEntry @208, MotionSensorSquadAssistScore: ScoreEntry @228, HotVehicleDestroyTimeout: Float32 @248, VehicleAirTimeLimit: Float32 @252, SuicideBecomesKillTimeLimit: Float32 @256, SuicideBecomesKillDamageThreshold: Float32 @260, StringForPinScore: String @264, DoublekillTime: Float32 @276, TriplekillTime: Float32 @280, MultikillTime: Float32 @284, Ranks: RankLevelData[] @288, DogTags: DogTagData[] @304, ScoringBuckets: ScoringBucketConfiguration @324, StringForStarScore: String @340, StringForInsigniaScore: String @352, NemesisKillCount: Float32 @364, DamageHistoryTime: Float32 @368, StrikeKillLimit: Uint32 @372, StrikeKillTimeLimit: Float32 @376, SquadKillLimit: Uint32 @380, SoldierAirTimeLimit: Float32 @384, DriverTimeLimit: Float32 @388, IngameTimeLimit: Float32 @392, ReviveScoreLimitAmount: Int32 @396, HealScoreLimitTime: Float32 @400, HealScoreLimitMultiplier: Float32 @404, ResupplyScoreLimitTime: Float32 @408, ResupplyScoreLimitAmount: Int32 @412, ReviveScoreLimitTime: Float32 @416, ObjectiveTeamDamageScore: ScoreEntry @420, SquadVehicleAssistScore: ScoreEntry @440, AvengerKillScore: ScoreEntry @460, SaviorKillScore: ScoreEntry @480, DriverAssistScore: ScoreEntry @500, SquadDriverAssistScore: ScoreEntry @520, VehicleAssistScore: ScoreEntry @540, DoubleKillScore: ScoreEntry @560, SquadAttackOrderScore: ScoreEntry @580, SquadDefendOrderScore: ScoreEntry @600, ParachuteKillScore: ScoreEntry @620, MultiKillScore: ScoreEntry @640, TripleKillScore: ScoreEntry @660, SuicideScore: ScoreEntry @680, AmmoBoxScore: ScoreEntry @700, SquadAmmoBoxScore: ScoreEntry @720, MeleeKillScore: ScoreEntry @740, MeleeKillWeapon: StatsCategoryData @760, ReviveScore: ScoreEntry @764, SquadReviveScore: ScoreEntry @784, HotVehicleDestroyScore: ScoreEntry @804, VehicleDamageScore: ScoreEntry @824, TeamKillScore: ScoreEntry @844, KillScore: ScoreEntry @864, HeadshotScore: ScoreEntry @884, MarksmanHeadshotScore: ScoreEntry @904, CrateDefendKillScore: ScoreEntry @924, SquadTaggedDamageAssistScore: ScoreEntry @944, TaggedKillAssistScore: ScoreEntry @964, SquadTaggedKillAssistScore: ScoreEntry @984, TaggedDestroyAssistScore: ScoreEntry @1004, SquadTaggedDestroyAssistScore: ScoreEntry @1024, TaggedDamageAssistScore: ScoreEntry @1044, DisarmObjectiveScore: ScoreEntry @1064, CapturedCapturePointScore: ScoreEntry @1084, CapturedCapturePointAssistScore: ScoreEntry @1104, TagScore: ScoreEntry @1124, ObjectiveDamageScore: ScoreEntry @1144, ArmObjectiveScore: ScoreEntry @1164, CriticalSquadKillAssistScore: ScoreEntry @1184, RepairTeamVehicleScore: ScoreEntry @1204, RepairSquadVehicleScore: ScoreEntry @1224, DamageTeamVehicleScore: ScoreEntry @1244, CrateAttackKillScore: ScoreEntry @1264, FlagDefenceScore: ScoreEntry @1284, SpotAssistScore: ScoreEntry @1304, KillAssistScore: ScoreEntry @1324, CriticalKillAssistScore: ScoreEntry @1344, SquadKillAssistScore: ScoreEntry @1364, TeamHealingScore: ScoreEntry @1384, TeamDamageScore: ScoreEntry @1404, SquadHealingScore: ScoreEntry @1424 |
| PositionEvaluationConstantData | class | DataContainer | 36 | FrontLineAccuracy: Uint32 @12, RandomPositionCount: Uint32 @16, OwnRandomPositionsRadius: Float32 @20, MaxCachedNodeIds: Uint32 @24, FrontLineSearchSensitivity: Uint32 @28, FrontLineSearchDepth: Uint32 @32 |
| PrimitiveConfigData | class | DataContainer | 12 | - |
| ProceduralLevelCreateDesc | class | Asset | 52 | OutputPath: String @24, OutputName: String @36, SaveToDiskEnable: Boolean @48 |
| SearchAndDestroyConstantData | class | DataContainer | 28 | KnownTargetRadius: Float32 @12, DefaultSearchAndDestroyRadius: Float32 @16, TimeToWaitMin: Float32 @20, TimeToWaitMax: Float32 @24 |
| SharedSoldierAnimationTreeData | class | DataContainer | 164 | HipBoneName: String @12, CameraBoneName: String @24, HeadBoneName: String @36, LowestBoneName: String @48, CameraFovBoneName: String @60, MovementCycleScaleFactor: Float32 @72, WeaponSwitchInfluenceMap: InfluenceMapData @76, OneClimbCycleDistance: Float32 @80, MinMoveSpeed: Float32 @84, Skeleton: SkeletonAsset @88, BoneCollision: SoldierBoneCollisionData @92, BaseSetAnimTrees: AnimTreeDataAsset[] @96, DeathAnimTree: AnimTreeDataAsset @116, LocalAnimations: IndexedAnimationData @120, UpperBodyRotation: SoldierBodyRotationData @124, WeaponBoneName: String @144, AIStateAnimation: AIStateAnimationData @156, RagdollSettings: AnimatedSoldierRagdollSettingsData @160 |
| SoldierAnimationTreeData | class | DataContainer | 60 | ReviveAnimTree: AnimTreeDataAsset @12, StaticJoints: StaticJointData[] @16, Shared: SharedSoldierAnimationTreeData @32, FacialAnimations: IndexedAnimationData @36, FacialSoundMap: FacialSoundMapData[] @40, KitJointsAnimTree: AnimTreeDataAsset @56 |
| SoldierAsset | class | ObjectBlueprint | 84 | - |
| SoldierAutoAimData | class | DataContainer | 28 | Poses: AutoAimData[] @12 |
| SoldierBoneCollisionData | class | DataContainer | 28 | BoneCollisionData: BoneCollisionData[] @12 |
| SoldierControlConstantData | class | DataContainer | 124 | AlertedNormalSpeed: Float32 @12, FastSpeed: Float32 @16, SprintSpeed: Float32 @20, RelaxedNormalSpeed: Float32 @24, MoveAngularAcceleration: Float32 @28, MoveAngularSpeedFactor: Float32 @32, SneakingSpeed: Float32 @36, MaxForwardDir: Float32 @40, MaxStrafeDir: Float32 @44, StandingStillCutoffSpeed: Float32 @48, AimStrafeSpeed: Float32 @52, CrouchSpeed: Float32 @56, CatchupSpeed: Float32 @60, AimForwardSpeed: Float32 @64, FastHeadTurningSpeed: Float32 @68, RelaxedTurningSpeed: Float32 @72, NormalTurningSpeed: Float32 @76, NormalHeadTurningSpeed: Float32 @80, MaximumHeadTurnAngle: Float32 @84, SearchHeadTurningSpeed: Float32 @88, SlowHeadTurningSpeed: Float32 @92, AimControlMode: AimControlMode @96, Acceleration: Float32 @100, Deceleration: Float32 @104, PitchSpeed: Float32 @108, CrouchTurningSpeed: Float32 @112, TurnAngularAcceleration: Float32 @116, AimMinPrecisionTolerance: Float32 @120 |
| SoldierHeadCollisionData | class | DataContainer | 32 | PoseStates: SoldierHeadCollisionPoseData[] @12, HeadRadius: Float32 @28 |
| SoldierParachuteData | class | DataContainer | 112 | ParachuteOffset3p: Vec3 @16, ParachuteOffset1p: Vec3 @32, TiltSpeed: Float32 @48, MaxTiltAtSpeed: Float32 @52, RandomTiltFrequency: Float32 @56, RandomMaxTiltAngle: Float32 @60, RandomTiltAxisOffset: Float32 @64, MaxTiltAngle: Float32 @68, ParachuteMesh: SkinnedMeshAsset @72, AnimTree: AnimTreeData @76, ParachuteMeshShaderSet: MeshShaderSetAsset @80, ParachuteTurningSpeed: Float32 @84, AimingConstraints: AimingConstraintsData @88 |
| SoldierSoundData | class | DataContainer | 136 | SprintSound: SoundAsset @12, SprintRecoverySound: SoundAsset @16, HealingFinishedSound: SoundAsset @20, BulletImpactsPauseTime: Float32 @24, HealingSound: SoundAsset @28, VegetationCollisionSound: SoundAsset @32, FootStepAnimTrigPositions3P: FootStepTrigger[] @36, SprintTimeForRecovery: Float32 @52, TimeBetweenTriggers: SoundTrigInfo @56, VegetationCollisionPauseTime: Float32 @76, VegetationCollisionVelocityThreshold: Float32 @80, BulletImpactSound: SoundAsset @84, FreeFallingSound: SoundAsset @88, ParachuteActiveSound: SoundAsset @92, JumpSound: SoundAsset @96, SwimSound: SoundAsset @100, ClimbSound: SoundAsset @104, ClothesSound: SoundAsset @108, DeathSound: SoundAsset @112, WeaponDeploySound: SoundAsset @116, BreathSound: SoundAsset @120, WeaponZoomSound: SoundAsset @124, ChangePoseSound: SoundAsset @128, BulletImpactsOnLocalSoldiersOnly: Boolean @132 |
| SoldierSpawnAppearanceTemplateData | class | Asset | 28 | Body: Uint32 @24 |
| SoldierSpawnKitTemplateData | class | Asset | 32 | MainGun: Uint32 @24, Kit: Uint32 @28 |
| SoldierSpawnTemplateData | class | Asset | 76 | Behaviour: BehaviourTemplateData @24, Rank: Uint32 @28, Sensing: SensingTemplateData @32, AllowedKits: SoldierSpawnKitTemplateData[] @36, AllowedAppearances: SoldierSpawnAppearanceTemplateData[] @56 |
| SoundAreaReverbs | class | Editable | 48 | AreaType: SoundAreaType @24, Reverbs: ReverbAsset[] @28 |
| SoundEnvironmentConstantData | class | DataContainer | 24 | MinimumLoudness: Float32 @12, MemoryTime: Float32 @16, HumanPlayerSoldierMovementVolumeOverride: Float32 @20 |
| SpawnCamera | class | CameraData | 64 | - |
| SpawnPointManagerData | class | Asset | 28 | UseAiLodSystem: Boolean @24 |
| StatCategoriesTree | class | TreeBase | 104 | ParamY: CriteriaData[] @24, General: CriteriaData[] @44, Categories: StatsCategoryData[] @64, ParamX: CriteriaData[] @84 |
| StaticCameraData | class | TargetCameraData | 432 | DownPitchAngle: Float32 @176, AverageFilterFrames: Int32 @180, RightYawAngle: Float32 @184, UpPitchAngle: Float32 @188, YawSensitivityNonZoomed: Float32 @192, PitchSensitivityZoomed: Float32 @196, SoundShakeFactor: Float32 @200, YawSensitivityZoomed: Float32 @204, PitchSensitivityNonZoomed: Float32 @208, ImpulseSettings: CameraImpulseShakeData @212, LeftCurve: Vec3[] @320, DownCurve: Vec3[] @336, RightCurve: Vec3[] @352, UpCurve: Vec3[] @368, YawInputAction: EntryInputActionEnum @384, LeftYawAngle: Float32 @388, PitchInputAction: EntryInputActionEnum @392, LoosePartPhysicsArray: LoosePartPhysicsData[] @396, ResetAccumulatedInputOnViewChange: Boolean @416, MirrorCurve: Boolean @417, AccumulateInput: Boolean @418 |
| StreamGroupData | class | DataContainer | 32 | Assets: Asset[] @12 |
| TankControlConstantData | class | DataContainer | 28 | SmoothTurnSpeedLimit: Float32 @12, TurnSpeedLimit: Float32 @16, YawStrength: Float32 @20, SmoothTurnSpeedAngle: Float32 @24 |
| TargetCameraData | class | CameraData | 176 | ImpactHudFxInfo: HudImpactData @64, CriticalHudFx: HudFxData @84, Hud: HudData @88 |
| TargetEvaluationConstantData | class | DataContainer | 36 | DefaultHumanTargetPreference: Float32 @12, OtherSquadAttackerFactorUsage: Float32 @16, TargetFlipFactorUsage: Float32 @20, KeepTargetTime: Float32 @24, MaxBattleStrength: Float32 @28, UpdateInterval: Float32 @32 |
| TeamAsset | class | Asset | 24 | - |
| TeamData | class | TeamAsset | 56 | MaxPlayerCount: Uint32 @24, MaxPlayersPerSquad: Uint32 @28, Kits: SoldierAsset[] @32, Faction: FactionId @52 |
| TimingConstantData | class | DataContainer | 76 | PriorityClasses: BotPriorityConfigData[] @12, SubsystemTimeParts: SubsystemTimingConfigData @32, UpdateTimePerFrame: Float32 @72 |
| TraversalConfigData | class | DataContainer | 32 | MaxDistance: Float32 @12, TimePart: Int32 @16, Name: String @20 |
| TraversalConstantData | class | DataContainer | 28 | EnvironmentTraversalConstants: TraversalConfigData @12, FrontLineTraversalConstants: TraversalConfigData @16, BehaviourTestTraversalConstants: TraversalConfigData @20, CoverSearchTraversalConstants: TraversalConfigData @24 |
| TreeHealthStateData | class | HealthStateData | 132 | ShakeEffects: EffectAsset[] @92, BranchEffects: EffectAsset[] @112 |
| TreeMemberData | class | GroupMemberData | 80 | PartsToBangerParts: StaticModelPartToBangerParts[] @60, MeshEntityType: TreeMeshEntityData @76 |
| TurretControlConstantData | class | DataContainer | 28 | NormalLookSpeed: Float32 @12, FastLookSpeed: Float32 @16, SearchLookSpeed: Float32 @20, SlowLookSpeed: Float32 @24 |
| TurretData | class | DataContainer | 32 | FakeShootSpaceMaxDistance: Float32 @12, AimingConstraints: AIAimingConstraintsData @16, FakeShootSpaceMinDistance: Float32 @20, PitchP: Float32 @24, YawP: Float32 @28 |
| VehicleAsset | class | ObjectBlueprint | 84 | - |
| VehicleWaterEffectData | class | DataContainer | 48 | Position: Vec3 @16, Effect: EffectAsset @32 |
| VoiceOverLineAsset | class | DataContainer | 24 | DialogLineNode: DialogLineNode @12, Soldier: SoldierSpawnBaseEntityData @16, OverRadio: Boolean @20 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27]

r"""effectport.py - Battlefield 1943 effects in BFBC2: an effect, the emitters it plays, the textures they draw; and
the sound porter the effects (and the WW2 levels) hand their sounds to.

    import effectport
    files, report = effectport.port(src, ix, 'effects/weapons/misc/effect_aa_detonationblast', 'levels/wake_island_s')
    # files: {partition / resource name (BF1943's spelling): PC bytes}; report: a dbxport.Report (lines, records)

What an effect is (both games): an EffectAsset partition holding an EffectEntityData whose components are
EmitterEntityData (each naming an emitter document) and SoundEntityData (a sound). BF1943 keeps the emitter documents
per level (levels/<level>/emitters/...: the partition inside keeps the global name emitters/...), each made of Emitter.*
data instances (spawn rate, size, colour curves, ...) and naming its textures (textures/effects/...).

How it is ported (the suite's Rust core since round OR8: rust/bc2-convert/src/effectport.rs and soundport.rs, through
bc2_native.effectport_* / soundport_port; the Python this module was is retired, see rust/BENCHMARKS.md "OR8"):
- every partition the effect reaches is walked (walk): emitter documents are taken from the level's copy (another
  level's where the level has none), textures are converted (the Rust texture converter), sounds and anything else
  BFBC2 has under the same name are kept as references;
- each partition goes through the Rust dbxport port on BFBC2 templates (TEMPLATES, then the first BFBC2 emitter
  documents in name order holding a type still needed - an index of their instance types kept on disk, the
  'effectport' cache; DEFAULTS checks the templates give DICE's usual values for the fields BFBC2 added);
- a BF1943 sound BFBC2 lacks is not ported here: port() reports it ('sound' records, sounds_of()) for the sound
  porter (port_sounds, which Project OPFOR's wake_level.port_sounds calls with its donors);
- a reference that will resolve to nothing is null and reported (dbxport: no template value stands in for it);
- the report is a dbxport.Report: its records hold every partition's dbxport records (sounds_of reads them).

The game data stays the caller's: a bf1943import.Source and a meshview.Index, or any object with their `entries` /
`data(key)` and `where` / `read(name)` (a view of the index that hides names, as OPFOR's WithoutOurs). The module is
required (bfbc2core.native.NativeMissing says how to get it).
"""
import os
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'core')
import dbxport  # noqa: E402
from bfbc2core import native  # noqa: E402

# BFBC2 partitions whose instances serve as templates first: an effect (DICE's small generic detonation); the emitter
# documents that cover the Emitter.* types follow, found by the search (the Rust core's effectport::TEMPLATES)
TEMPLATES = ['fx/explostions/detonations/generic/fx_detonation_s']
# the DICE-only fields whose values vary over DICE's own effects (dbxparity.py): the value the templates must give
# (the Rust core's effectport::DEFAULTS: a ported partition with another value gets a 'default' record)
DEFAULTS = {('EmitterEntityData', 'SpawnProbability'): '1', ('EffectEntityData', 'TriggerPostEffect'): 'false',
            ('EffectEntityData', 'PostEffectContrastOffset'): '0', ('EffectEntityData', 'PostEffectFadeInTime'): '0',
            ('EffectEntityData', 'PostEffectFadeOutTime'): '0', ('EffectEntityData', 'PostEffectSizeScale'): '1'}


def _rust() -> Any:
    return native.require('effectport (BF1943 effects and sounds)', 'soundport_port')


def _tables(catalogue: Any) -> tuple[Any, Any, Any]:
    """(the dbxport rules as the process has them, the catalogue's Rust copy, its class defaults) for the Rust port."""
    m = _rust()
    rules = dbxport._native_rules(m)
    if rules is None:
        raise dbxport.PortError('dbxport\'s tables hold what the Rust port does not take')
    if catalogue is None:
        return rules, None, None
    return rules, dbxport._native_catalogue(m, catalogue), catalogue.defaults


def _raise(kind: str, payload: Any) -> None:
    """The Rust core's error as the Python raised it."""
    if kind == 'port':
        raise dbxport.PortError(payload)
    if kind == 'schema':
        raise dbxport.SchemaError(payload)
    if kind == 'missing_template':
        raise dbxport.MissingTemplate(payload)
    if kind == 'texture':
        import texture
        raise texture.ConvertError(payload)
    if kind == 'exit':
        raise SystemExit(payload)
    if kind == 'io':
        raise OSError(payload)
    if kind == 'unsupported':
        raise dbxport.PortError(f'the Rust port does not take this partition: {payload}')
    raise ValueError(payload)


def _ported(got: tuple) -> tuple[dict[str, bytes], dbxport.Report]:
    if got[0] == 'error':
        _raise(got[1], got[2])
    _, files, lines, records = got
    report = dbxport.Report(lines)
    report.records = [dbxport.Record(*r) for r in records]
    return dict(files), report


def levels_of(src: Any) -> list[str]:
    """BF1943's level folders (levels/<name>) that hold level copies of emitter documents, sorted."""
    return list(_rust().effectport_levels(src))


def walk(src: Any, ix: Any, effect: str, level: str, fallback: bool = True) -> list[tuple[str, str | None, str]]:
    """[(partition name, source key, kind)] the effect reaches: kind 'port' (BF1943's, BFBC2 lacks it), 'texture',
    'bfbc2' (kept as a reference), 'sound' (BF1943's, BFBC2 lacks it: for the sound porter), 'missing'. An emitter
    document is the level's copy (another level's when the level has none and fallback is set - the source key says
    which)."""
    return list(_rust().effectport_walk(src, ix, effect, level, fallback))


def templates(ix: Any, types: Any) -> tuple[list[str], list[str]]:
    """(the template pool, the types none of it holds) the search finds for the instance types given."""
    pool, need, _ = _rust().effectport_templates(ix, sorted(types), _tables(None)[0])
    return pool, need


def port(src: Any, ix: Any, effect: str, level: str, catalogue: Any = None,
         schema: str = 'error') -> tuple[dict[str, bytes], dbxport.Report]:
    """({file: bytes}, dbxport.Report) of one effect. The report's records: the effect's summary ('summary'), each
    partition's dbxport records (their partition set), 'sound' (a BF1943 sound for the caller's sound porter: its
    path), 'missing', 'default', 'level', 'types'; its lines: the said ones and the dbxport lines of the actions shown
    (substituted, nulled, relocated, ...), indented under the partition. schema: dbxport's ('report': a shape no
    converter handles is kept as BF1943's and reported, 'schema' records)."""
    rules, cat, defaults = _tables(catalogue)
    return _ported(_rust().effectport_port(src, ix, effect, level, rules, cat, defaults, schema))


def port_many(src: Any, ix: Any, effects: list[str], level: str, log: Any = print, catalogue: Any = None,
              schema: str = 'error') -> tuple[dict[str, bytes], dbxport.Report]:
    """{file: bytes}, report for several effects - the templates found once for all of them (each effect's own
    search reads BFBC2's emitter documents: slow for dozens)."""
    rules, cat, defaults = _tables(catalogue)
    return _ported(_rust().effectport_port_many(src, ix, list(effects), level, rules, cat, defaults, schema, log))


def sounds_of(report: Any) -> list[str]:
    """The BF1943 sound partitions port() / port_many() left for a sound porter (their 'sound' records; a report
    from before records - a cache - by its SOUND lines)."""
    records = getattr(report, 'records', None)
    if records is not None:
        return sorted({r.path for r in records if r.action == 'sound'})
    return sorted({r.split(' ', 2)[1][:-1] for r in report if r.startswith('SOUND ')})


def port_sounds(src: Any, ix: Any, roots: list[str], out: str, *, donors: list[str], last_donors: list[str],
                follow: tuple[str, ...] = ('sound/',), stand_ins: dict[str, str] | None = None,
                later: dict[str, str] | None = None, catalogue: Any = None, log: Any = print) -> list[str]:
    """Ports the BF1943 sound partitions under roots (and those they name under the follow prefixes) that BFBC2 lacks
    into out (an install root), their waves converted (EA-XMA -> EA-XAS, in memory: a streamed wave's audio from its
    _waveDataSns_ entries, its IsStreaming then false); returns the partition names ported, in the order found.
    donors: the BFBC2 sound partitions to take templates from (with the sound partitions they name); last_donors:
    templates for the types the pool lacks, taken last. The fields BFBC2's sound classes have and BF1943's lack are
    set as DICE set them when it converted BF1943's sounds. An instance of a type no template has (kept in BF1943's
    schema) stops the port (SystemExit) - unless its partition is in later ({partition: the step that ports it
    again}): then it is said."""
    rules, cat, defaults = _tables(catalogue)
    got = _rust().soundport_port(src, ix, list(roots), str(out), list(donors), list(last_donors), list(follow),
                                 dict(stand_ins or {}), dict(later or {}), rules, cat, defaults, log)
    if got[0] == 'error':
        _raise(got[1], got[2])
    return list(got[1])


def sound_fills(data: bytes, source: bytes) -> tuple[bytes, int]:
    """A ported sound partition with the fields BFBC2's sound classes have and BF1943's lack set as DICE did, on the
    instances whose BF1943 version (source) lacks them: (data, number of fields changed)."""
    d, n = _rust().soundport_fills(bytes(data), bytes(source))
    return d, n


def not_streaming(data: bytes) -> tuple[bytes, bool]:
    """A wave partition with its WaveAsset's IsStreaming false: (data, whether it changed)."""
    d, changed = _rust().soundport_not_streaming(bytes(data))
    return d, changed

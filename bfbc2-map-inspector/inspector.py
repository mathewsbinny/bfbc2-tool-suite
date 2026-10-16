"""BFBC2 Map Inspector: browse a level in 3D and inspect every object's source and properties.

  python inspector.py <level>                 e.g. inspector.py sp_01   (the level must be NavKit-prepared)
  python inspector.py <level> --screenshot out.png [--select <guid>] [--frames N]   draw, save, exit (tests)

Click an object to select it, double-click to fly the camera to it. Left drag rotates, right or middle drag
pans, the wheel zooms; F switches to fly mode (hold the right button, W A S D, Q / E, Shift faster).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 'core'))

import numpy as np  # noqa: E402
from imgui_bundle import imgui  # noqa: E402

from bfbc2core import dbx, level as levelmod, viewer  # noqa: E402

TERRAIN_ID = 0xFFFFFE
# layer: colour, marker size for entities without collision (None: not drawn without collision)
LAYERS = [
    ('terrain', (0.42, 0.47, 0.36), None),
    ('placed object', (0.62, 0.60, 0.56), 1.0),
    ('group member', (0.55, 0.53, 0.50), None),
    ('soldier spawn', (0.20, 0.65, 1.00), 1.2),
    ('vehicle spawn', (0.15, 0.85, 0.75), 2.5),
    ('objective', (1.00, 0.35, 0.20), 2.0),
    ('capture point', (1.00, 0.75, 0.10), 2.0),
    ('combat area', (0.85, 0.25, 0.85), 1.5),
    ('ai order', (0.55, 0.90, 0.30), 1.0),
    ('area point', (0.70, 0.70, 0.95), 0.6),
    ('other', (0.60, 0.60, 0.60), 0.6),
]
HIDDEN_AT_START = {'area point', 'other'}


class Inspector:
    def __init__(self, lvl):
        self.lvl = lvl
        self.scene = viewer.Scene()
        self.layer_index = {}
        for name, colour, _ in LAYERS:
            self.layer_index[name] = self.scene.layer(name, colour)
        sizes = {name: size for name, _, size in LAYERS}
        with_mesh = set()
        for m in lvl.meshes:
            e = lvl.entities[m.entity]
            self.scene.add_triangles(m.triangles, e.index, self.layer_index.get(e.category, self.layer_index['other']))
            with_mesh.add(e.index)
        for e in lvl.entities:
            if e.index in with_mesh or e.position is None:
                continue
            size = sizes.get(e.category, 0.6)
            if size:
                self.scene.add_box(e.position + np.array([0, size / 2, 0]), size, e.index,
                                   self.layer_index.get(e.category, self.layer_index['other']))
        if lvl.terrain is not None:
            self.scene.add_indexed(lvl.terrain[0], lvl.terrain[1], TERRAIN_ID, self.layer_index['terrain'])
        self.counts = {}
        for e in lvl.entities:
            self.counts[e.category] = self.counts.get(e.category, 0) + 1
        self.selected = None
        self.search = ''
        self.results = []
        self.history = []
        self.guids = {e.guid.lower(): e for e in lvl.entities}

    # --- selection ----------------------------------------------------------------------------------
    def select(self, v, entity, focus=False):
        self.selected = entity
        v.selected = entity.index if entity is not None else viewer.NO_ID
        if entity is not None and focus and entity.position is not None:
            v.camera.focus(entity.position, 40.0)

    def on_pick(self, v):
        def handler(oid):
            if oid is None or oid == TERRAIN_ID or oid >= len(self.lvl.entities):
                self.selected = None
            else:
                self.selected = self.lvl.entities[oid]
        return handler

    # --- panels -------------------------------------------------------------------------------------
    def panel_layers(self, v):
        imgui.set_next_window_pos((10, 10), imgui.Cond_.first_use_ever)
        imgui.set_next_window_size((300, 520), imgui.Cond_.first_use_ever)
        imgui.begin('Layers and search')
        imgui.text(f'{self.lvl.name}: {len(self.lvl.entities)} entities')
        imgui.separator()
        for name, colour, _ in LAYERS:
            i = self.layer_index[name]
            imgui.color_button(f'##c{name}', imgui.ImVec4(*colour, 1.0), 0, (14, 14))
            imgui.same_line()
            changed, on = imgui.checkbox(f'{name} ({self.counts.get(name, 1 if name == "terrain" else 0)})', v.layer_visible(i))
            if changed:
                v.set_layer_visible(i, on)
        imgui.separator()
        changed, self.search = imgui.input_text('search', self.search)
        if changed:
            q = self.search.lower().strip()
            self.results = [] if len(q) < 2 else [e for e in self.lvl.entities if q in (e.guid.lower() + ' ' + e.type.lower() + ' ' + (e.id or '').lower()
                                                                                          + ' ' + (e.name or '').lower() + ' ' + (e.blueprint or ''))][:300]
        imgui.text_disabled('GUID, type, id, name or blueprint')
        imgui.begin_child('results', (0, 0))
        for e in self.results:
            label = f'{e.short_type}  {e.id or e.name or e.blueprint or ""}##{e.index}'
            if imgui.selectable(label, self.selected is e)[0]:
                self.select(v, e, focus=True)
        imgui.end_child()
        imgui.end()

    def tree(self, v, el, path):
        """Property tree of an element: containers expand, values show inline, local references are links."""
        for k, c in enumerate(el.children or ()):
            name = c.attr('name') or c.tag
            ref = c.attr('ref')
            if c.children is not None:
                label = f'{name}  [{len(c.children)}]##{path}/{k}'
                if imgui.tree_node(label):
                    self.tree(v, c, f'{path}/{k}')
                    imgui.tree_pop()
            elif ref is not None:
                target = self.guids.get(ref.lower())
                if target is not None:
                    if imgui.small_button(f'-> {name}##{path}/{k}'):
                        self.history.append(self.selected)
                        self.select(v, target, focus=True)
                    imgui.same_line()
                    imgui.text_disabled(f'{target.short_type} {target.id or ""}')
                else:
                    imgui.text(f'{name} -> {ref}')
            else:
                val = dbx.value(c)
                imgui.text(f'{name} = {val if len(val) < 140 else val[:140] + " ..."}')

    def panel_selection(self, v):
        w = imgui.get_io().display_size.x
        imgui.set_next_window_pos((w - 470, 10), imgui.Cond_.first_use_ever)
        imgui.set_next_window_size((460, 700), imgui.Cond_.first_use_ever)
        imgui.begin('Selection')
        e = self.selected
        if e is None:
            imgui.text_disabled('Click an object (double-click: fly to it).')
        else:
            if self.history and imgui.small_button('< back'):
                self.select(v, self.history.pop(), focus=True)
            imgui.text(e.short_type)
            imgui.text_disabled(e.type)
            for label, value in (('guid', e.guid), ('id', e.id), ('name', e.name), ('category', e.category),
                                 ('blueprint', e.blueprint), ('source', e.partition + (f'  ({e.group})' if e.group else ''))):
                if value:
                    imgui.text(f'{label}: {value}')
            if e.position is not None:
                p = e.position
                imgui.text(f'position: {p[0]:.2f}, {p[1]:.2f}, {p[2]:.2f}')
            if imgui.button('fly to'):
                self.select(v, e, focus=True)
            imgui.same_line()
            if imgui.button('copy guid'):
                imgui.set_clipboard_text(e.guid)
            imgui.separator()
            imgui.begin_child('props', (0, 0))
            self.tree(v, e.element, str(e.index))
            imgui.end_child()
        imgui.end()

    def panel_status(self, v):
        io = imgui.get_io()
        imgui.set_next_window_pos((10, io.display_size.y - 40), imgui.Cond_.always)
        imgui.set_next_window_size((io.display_size.x - 20, 30), imgui.Cond_.always)
        imgui.begin('##status', flags=imgui.WindowFlags_.no_decoration | imgui.WindowFlags_.no_inputs | imgui.WindowFlags_.no_background)
        hov = ''
        if v.hovered not in (viewer.NO_ID, TERRAIN_ID) and v.hovered < len(self.lvl.entities):
            h = self.lvl.entities[v.hovered]
            hov = f'   under the mouse: {h.short_type} {h.id or h.name or h.blueprint or ""}'
        elif v.hovered == TERRAIN_ID:
            hov = '   under the mouse: terrain'
        if v.hover_point is not None:
            p = v.hover_point
            hov += f'  ({p[0]:.1f}, {p[1]:.1f}, {p[2]:.1f})'
        imgui.text(f'{v.fps:5.1f} fps   {self.scene_triangles:,} triangles   camera: {"fly" if v.camera.fly else "orbit"} (F switches){hov}')
        imgui.end()


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    opts = dict(zip(argv[1:], argv[2:]))
    if not args:
        print(__doc__)
        return 1
    lvl = levelmod.load(args[0])
    ins = Inspector(lvl)
    ins.scene_triangles = ins.scene.triangle_count()
    v = viewer.Viewer(ins.scene, f'BFBC2 Map Inspector - {lvl.name}',
                      panels=[ins.panel_layers, ins.panel_selection, ins.panel_status])
    v.on_pick = ins.on_pick(v)
    for name in HIDDEN_AT_START:
        v.set_layer_visible(ins.layer_index[name], False)
    if '--select' in opts:
        e = lvl.by_guid(opts['--select'])
        if e is not None:
            ins.select(v, e, focus=True)
    if '--screenshot' in opts:
        v.exit_after = int(opts.get('--frames', 40))
    v.run()
    if '--screenshot' in opts and v.screenshot is not None:
        import matplotlib.pyplot as plt
        plt.imsave(opts['--screenshot'], np.asarray(v.screenshot))
        print('screenshot:', opts['--screenshot'])
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))

// ApplyTypes.java - make the spec's reflection types and tie them to the classes RTTI and the reflection name (DC0).
// Args: <types_plan.json from tools/decomp/gen_types.py> <report.json>
//
//  1. Classes. Every vftable label the MSVC RTTI analyser made gives a class (Kynapse and a few libraries: most
//     engine classes carry no RTTI). Frostbite's reflection gives the rest: the registrations (RTTI-REG-1) map each
//     class info to its record's name; each class's getType (`mov eax, <class info>; ret`, RTTI-FAC-1) is found by
//     its bytes, a data word pointing at it is a vtable slot, and the vtable starts at the slot a constructor's code
//     references (labelled `<Class>::vftable`). Each vtable's slots are read (pointers into executable memory, up to
//     the next label or code-referenced word) and a function is made where a slot points at code with none. A
//     function in exactly one class's vtables (or in several, of which one is the others' ancestor by the plan's
//     parents), or one that stores only that class's vftables (`mov [reg+n], vftable`: constructors and destructors;
//     the most derived when several), is moved into the class (__thiscall).
//  2. Types. Every type of the plan is made in the category of the class of the same name (so that Ghidra types the
//     `this` of that class's functions with it), or in /BFBC2/typeinfo when no class has the name.
//  3. Report: classes, vtables, functions placed, types made and matched.
//@category BFBC2.Decomp
import com.google.gson.*;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.data.Enum;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.scalar.Scalar;
import ghidra.program.model.symbol.*;
import ghidra.program.model.util.CodeUnitInsertionException;
import ghidra.app.util.NamespaceUtils;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class ApplyTypes extends GhidraScript {
    Map<String, DataType> made = new HashMap<>();
    DataTypeManager dtm;
    CategoryPath fallback = new CategoryPath("/BFBC2/typeinfo");
    int membersPlaced = 0, membersSkipped = 0;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        JsonObject plan = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();
        dtm = currentProgram.getDataTypeManager();
        Map<String, Object> rep = new TreeMap<>();

        // ---- 1. classes from RTTI vftables
        SymbolTable st = currentProgram.getSymbolTable();
        Memory mem = currentProgram.getMemory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Map<Address, Namespace> vtClass = new HashMap<>();
        for (Symbol s : st.getAllSymbols(false)) {
            if (monitor.isCancelled()) break;
            String n = s.getName();
            if (!(n.equals("vftable") || n.startsWith("vftable{"))) continue;
            Namespace ns = s.getParentNamespace();
            if (ns == null || ns.isGlobal()) continue;
            vtClass.put(s.getAddress(), ns);
        }
        int rttiVt = vtClass.size();
        // ---- 1b. classes from Frostbite's reflection (most engine classes carry no MSVC RTTI). RTTI-REG-1: each
        // record is registered by `push <record>; mov ecx, <class info>; call <register>`; RTTI-FAC-1: each class's
        // vtable holds its getType, `mov eax, <class info>; ret`. The getType's data references are vtable slots; a
        // vtable starts at the slot a constructor's code references.
        Map<String, String> parentOf = new HashMap<>();
        for (JsonElement je : plan.getAsJsonArray("types")) {
            JsonObject t = je.getAsJsonObject();
            if (t.has("parent") && !t.get("parent").isJsonNull()) parentOf.put(t.get("name").getAsString(), t.get("parent").getAsString());
        }
        ReferenceManager rm0 = currentProgram.getReferenceManager();
        Map<Long, String> infoName = new HashMap<>();
        long[] registers = {0x500880L, 0x500660L, 0x43C390L, 0x5008E0L, 0x4FE260L};
        for (long reg : registers) {
            for (Reference r : rm0.getReferencesTo(toAddr(reg))) {
                if (!r.getReferenceType().isCall()) continue;
                Instruction ins = getInstructionAt(r.getFromAddress());
                Long info = null, record = null;
                for (int k = 0; k < 10 && ins != null && (info == null || record == null); k++) {
                    ins = ins.getPrevious();
                    if (ins == null) break;
                    String mn = ins.getMnemonicString();
                    if (info == null && mn.equals("MOV") && ins.getDefaultOperandRepresentation(0).equals("ECX")
                            && ins.getScalar(1) != null) info = ins.getScalar(1).getUnsignedValue();
                    else if (record == null && mn.equals("PUSH") && ins.getScalar(0) != null) record = ins.getScalar(0).getUnsignedValue();
                }
                if (info == null || record == null) continue;
                try {
                    Address np = toAddr(mem.getInt(toAddr(record)) & 0xFFFFFFFFL);
                    StringBuilder sb = new StringBuilder();
                    for (int k = 0; k < 128; k++) { byte bt = mem.getByte(np.add(k)); if (bt == 0) break; sb.append((char) bt); }
                    if (sb.length() > 0) infoName.put(info, sb.toString());
                } catch (Exception x) { }
            }
        }
        int fbVt = 0, getTypes = 0;
        // getType functions: `B8 <class info> C3` anywhere in executable memory (they need not be Ghidra functions yet)
        Map<Long, String> getTypeName = new HashMap<>();
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isExecute() || !b.isInitialized()) continue;
            byte[] buf = new byte[(int) b.getSize()];
            b.getBytes(b.getStart(), buf);
            for (int i = 0; i + 5 < buf.length; i++) {
                if (buf[i] != (byte) 0xB8 || buf[i + 5] != (byte) 0xC3) continue;
                long imm = (buf[i + 1] & 0xFFL) | (buf[i + 2] & 0xFFL) << 8 | (buf[i + 3] & 0xFFL) << 16 | (buf[i + 4] & 0xFFL) << 24;
                String n = infoName.get(imm);
                if (n != null) getTypeName.put(b.getStart().getOffset() + i, n);
            }
        }
        getTypes = getTypeName.size();
        // vtable slots: aligned words of the data sections pointing at a getType
        for (MemoryBlock b : mem.getBlocks()) {
            if (b.isExecute() || !b.isInitialized() || b.getSize() > (64L << 20)) continue;
            byte[] buf = new byte[(int) b.getSize()];
            b.getBytes(b.getStart(), buf);
            long base = b.getStart().getOffset();
            for (int i = (int) ((4 - (base & 3)) & 3); i + 4 <= buf.length; i += 4) {
                long v = (buf[i] & 0xFFL) | (buf[i + 1] & 0xFFL) << 8 | (buf[i + 2] & 0xFFL) << 16 | (buf[i + 3] & 0xFFL) << 24;
                String cls = getTypeName.get(v);
                if (cls == null) continue;
                Address slot = toAddr(base + i);
                // walk back to the slot a constructor's code references
                Address start = null, q = slot;
                for (int k = 0; k < 600; k++) {
                    if (codeRef(q)) { start = q; break; }
                    Address prev = q.subtract(4);
                    long t;
                    try { t = mem.getInt(prev) & 0xFFFFFFFFL; } catch (Exception x) { break; }
                    MemoryBlock tb;
                    try { tb = mem.getBlock(toAddr(t)); } catch (Exception x) { tb = null; }
                    if (tb == null || !tb.isExecute()) break;
                    q = prev;
                }
                if (start == null || vtClass.containsKey(start)) continue;
                try {
                    Namespace ns = st.getNamespace(cls, currentProgram.getGlobalNamespace());
                    if (ns == null) ns = st.createClass(currentProgram.getGlobalNamespace(), cls, SourceType.USER_DEFINED);
                    else if (!(ns instanceof GhidraClass)) ns = NamespaceUtils.convertNamespaceToClass(ns);
                    boolean has = false;
                    for (Symbol s : st.getSymbols(start)) if (s.getName().startsWith("vftable")) has = true;
                    if (!has) st.createLabel(start, "vftable", ns, SourceType.USER_DEFINED);
                    vtClass.put(start, ns);
                    fbVt++;
                } catch (Exception x) { }
            }
        }
        rep.put("reflection_class_infos", infoName.size());
        rep.put("reflection_getType_functions", getTypes);
        rep.put("reflection_vftables", fbVt);
        Map<Function, Set<Namespace>> inVt = new HashMap<>();
        int slots = 0, created = 0;
        for (Map.Entry<Address, Namespace> e : vtClass.entrySet()) {
            Address a = e.getKey();
            for (int i = 0; i < 4096; i++) {
                Address p = a.add(4L * i);
                if (i > 0 && st.getPrimarySymbol(p) != null && st.getPrimarySymbol(p).getSource() != SourceType.DEFAULT) break;
                if (i > 0 && vtClass.containsKey(p)) break;
                if (i > 0 && codeRef(p)) break;              // the next vtable (a constructor stores it)
                long t;
                try { t = mem.getInt(p) & 0xFFFFFFFFL; } catch (MemoryAccessException x) { break; }
                Address ta;
                try { ta = toAddr(t); } catch (Exception x) { break; }
                MemoryBlock b = mem.getBlock(ta);
                if (b == null || !b.isExecute()) break;
                Function f = fm.getFunctionAt(ta);
                if (f == null && fm.getFunctionContaining(ta) == null) {
                    if (getInstructionAt(ta) == null) disassemble(ta);
                    if (getInstructionAt(ta) != null) {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(ta);
                        if (cmd.applyTo(currentProgram, monitor)) { f = fm.getFunctionAt(ta); created++; }
                    }
                }
                if (f == null) break;
                slots++;
                inVt.computeIfAbsent(f, k -> new HashSet<>()).add(e.getValue());
            }
        }
        // constructors / destructors: functions storing vftable addresses
        Map<Function, Set<Namespace>> stores = new HashMap<>();
        ReferenceManager rm = currentProgram.getReferenceManager();
        for (Map.Entry<Address, Namespace> e : vtClass.entrySet()) {
            for (Reference r : rm.getReferencesTo(e.getKey())) {
                Instruction ins = getInstructionAt(r.getFromAddress());
                if (ins == null || !ins.getMnemonicString().equals("MOV") || r.getOperandIndex() != 1) continue;
                if (!ins.getDefaultOperandRepresentation(0).contains("[")) continue;
                Function f = fm.getFunctionContaining(ins.getAddress());
                if (f != null) stores.computeIfAbsent(f, k -> new HashSet<>()).add(e.getValue());
            }
        }
        int placed = 0, placedVirtual = 0, placedCtor = 0, shared = 0, sharedPlaced = 0;
        Set<Function> all = new HashSet<>(inVt.keySet());
        all.addAll(stores.keySet());
        for (Function f : all) {
            Set<Namespace> v = inVt.getOrDefault(f, Set.of());
            Set<Namespace> c = stores.getOrDefault(f, Set.of());
            Namespace target = null;
            if (v.size() == 1) { target = v.iterator().next(); }
            else if (v.size() > 1) {
                target = pick(v, parentOf, true);          // inherited: the ancestor of all of them
                if (target == null) { shared++; continue; }
                sharedPlaced++;
            }
            else if (c.size() == 1) { target = c.iterator().next(); }
            else if (c.size() > 1) target = pick(c, parentOf, false);   // a constructor: the most derived
            if (target == null || f.isThunk() || f.isExternal()) continue;
            if (!f.getParentNamespace().isGlobal()) continue;
            try {
                Namespace cls = target instanceof GhidraClass ? target : NamespaceUtils.convertNamespaceToClass(target);
                f.setParentNamespace(cls);
                if (f.getSymbol().getSource() == SourceType.DEFAULT || f.getCallingConventionName().equals("unknown")
                        || f.getCallingConventionName().equals("__stdcall") || f.getCallingConventionName().equals("default"))
                    f.setCallingConvention("__thiscall");
                placed++;
                if (!v.isEmpty()) placedVirtual++; else placedCtor++;
            } catch (Exception x) { /* name clash etc.: leave the function where it is */ }
        }
        rep.put("rtti_vftables", rttiVt);
        rep.put("vftables", vtClass.size());
        rep.put("functions_in_several_vtables_placed_by_parent", sharedPlaced);
        rep.put("vtable_slots", slots);
        rep.put("functions_created_from_vtables", created);
        rep.put("functions_placed_in_class", placed);
        rep.put("functions_placed_virtual", placedVirtual);
        rep.put("functions_placed_ctor_dtor", placedCtor);
        rep.put("functions_in_several_vtables", shared);

        // ---- 2. types
        Map<String, List<GhidraClass>> classes = new HashMap<>();
        for (Symbol s : st.getDefinedSymbols()) {
            if (s.getSymbolType() != SymbolType.CLASS && s.getSymbolType() != SymbolType.NAMESPACE) continue;
            Object o = s.getObject();
            if (o instanceof GhidraClass gc) classes.computeIfAbsent(s.getName(), k -> new ArrayList<>()).add(gc);
        }
        JsonArray types = plan.getAsJsonArray("types");
        Map<String, CategoryPath> cat = new HashMap<>();
        int matched = 0;
        for (JsonElement je : types) {
            String name = je.getAsJsonObject().get("name").getAsString();
            List<GhidraClass> l = classes.get(name);
            if (l != null && l.size() == 1) {
                Namespace parent = l.get(0).getParentNamespace();
                String path = parent.isGlobal() ? "/" : "/" + parent.getName(true).replace("::", "/");
                cat.put(name, new CategoryPath(path));
                matched++;
            } else cat.put(name, fallback);
        }
        int tx = currentProgram.startTransaction("DC0 types");   // headless scripts run inside one already; harmless
        try {
            Category old = dtm.getCategory(fallback);              // an earlier run's types: made again below
            if (old != null) for (DataType d : old.getDataTypes()) dtm.remove(d, monitor);
            // placeholders first, so pointers can name any structure
            for (JsonElement je : types) {
                JsonObject t = je.getAsJsonObject();
                String name = t.get("name").getAsString();
                int size = t.get("size").getAsInt();
                CategoryPath cp = cat.get(name);
                DataType dt;
                if (t.get("kind").getAsString().equals("enum")) {
                    EnumDataType en = new EnumDataType(cp, name, Math.max(1, Math.min(8, size)), dtm);
                    for (JsonElement v : t.getAsJsonArray("values")) {
                        JsonArray va = v.getAsJsonArray();
                        try { en.add(va.get(0).getAsString(), va.get(1).getAsLong()); } catch (Exception x) { }
                    }
                    dt = en;
                } else {
                    dt = new StructureDataType(cp, name, size, dtm);
                    if (t.has("note")) ((Structure) dt).setDescription("spec/typeinfo.json: " + t.get("note").getAsString());
                }
                DataType existing = dtm.getDataType(cp, name);
                if (existing instanceof Structure es && dt instanceof Structure) {
                    es.replaceWith(dt);
                    made.put(name, es);
                } else {
                    made.put(name, dtm.addDataType(dt, DataTypeConflictHandler.REPLACE_HANDLER));
                }
            }
            for (JsonElement je : types) {
                JsonObject t = je.getAsJsonObject();
                if (!t.get("kind").getAsString().equals("struct")) continue;
                Structure s = (Structure) made.get(t.get("name").getAsString());
                for (JsonElement me : t.getAsJsonArray("members")) {
                    JsonObject m = me.getAsJsonObject();
                    DataType ft = resolve(m.getAsJsonObject("type"));
                    int off = m.get("offset").getAsInt();
                    if (ft == null || off < 0 || off + ft.getLength() > s.getLength()) { membersSkipped++; continue; }
                    try { s.replaceAtOffset(off, ft, ft.getLength(), m.get("name").getAsString(), null); membersPlaced++; }
                    catch (Exception x) { membersSkipped++; }
                }
            }
        } finally { currentProgram.endTransaction(tx, true); }
        // `this` of the matched classes' functions
        int typedThis = 0;
        for (Map.Entry<String, List<GhidraClass>> e : classes.entrySet()) {
            if (e.getValue().size() != 1 || !made.containsKey(e.getKey())) continue;
            GhidraClass gc = e.getValue().get(0);
            for (Symbol s : st.getSymbols(gc)) {
                if (s.getSymbolType() != SymbolType.FUNCTION) continue;
                Function f = (Function) s.getObject();
                if (f.getCallingConventionName().equals("__thiscall")) typedThis++;
            }
        }
        rep.put("types_made", made.size());
        rep.put("types_matched_to_rtti_class", matched);
        rep.put("members_placed", membersPlaced);
        rep.put("members_skipped", membersSkipped);
        rep.put("thiscall_functions_of_typed_classes", typedThis);
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[1]), StandardCharsets.UTF_8))) {
            out.println(new GsonBuilder().setPrettyPrinting().create().toJson(rep));
        }
        println("ApplyTypes: " + rep);
    }

    boolean codeRef(Address a) {
        for (Reference r : currentProgram.getReferenceManager().getReferencesTo(a))
            if (getInstructionAt(r.getFromAddress()) != null) return true;
        return false;
    }

    static boolean descends(String c, String anc, Map<String, String> parentOf) {
        for (int k = 0; c != null && k < 64; k++) { if (c.equals(anc)) return true; c = parentOf.get(c); }
        return false;
    }

    /** The class of the set every other one descends from (base) or that descends from every other (!base). */
    static Namespace pick(Set<Namespace> set, Map<String, String> parentOf, boolean base) {
        for (Namespace cand : set) {
            boolean ok = true;
            for (Namespace o : set) {
                if (o == cand) continue;
                if (base ? !descends(o.getName(), cand.getName(), parentOf) : !descends(cand.getName(), o.getName(), parentOf)) { ok = false; break; }
            }
            if (ok) return cand;
        }
        return null;
    }

    DataType resolve(JsonObject t) {
        String k = t.get("kind").getAsString(), n = t.get("name").getAsString();
        switch (k) {
            case "builtin":
                if (n.equals("char *")) return new PointerDataType(CharDataType.dataType, 4, dtm);
                DataType b = switch (n) {
                    case "bool" -> BooleanDataType.dataType; case "sbyte" -> SignedByteDataType.dataType;
                    case "byte" -> ByteDataType.dataType; case "short" -> ShortDataType.dataType;
                    case "ushort" -> UnsignedShortDataType.dataType; case "int" -> IntegerDataType.dataType;
                    case "uint" -> UnsignedIntegerDataType.dataType; case "longlong" -> LongLongDataType.dataType;
                    case "ulonglong" -> UnsignedLongLongDataType.dataType; case "float" -> FloatDataType.dataType;
                    case "double" -> DoubleDataType.dataType; default -> null; };
                return b;
            case "pointer": {
                DataType d = made.get(n);
                return d == null ? null : new PointerDataType(d, 4, dtm);
            }
            default:
                return made.get(n);
        }
    }
}

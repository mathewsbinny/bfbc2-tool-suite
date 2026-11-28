// ConfirmFunctions.java - how each function was found, and whether anything independent references the doubtful ones
// (round DC0). Read only.
// Args: <discovered.jsonl from DiscoverFunctions.java> <provenance.jsonl to write> <junk first hex> <junk last hex>
//
// A function is "analysis" (Ghidra's auto-analysis or the vtable pass of ApplyTypes.java), "orphan" (made from code
// analysis had disassembled but put in no function) or "prologue" (made from never-disassembled bytes by a prologue
// scan). Doubtful: every prologue function, and every function whose entry lies in the junk range (code analysis
// disassembled out of bytes that are probably not code). A doubtful function is confirmed when its entry is
//   - called or jumped to from a trusted function (not doubtful, or doubtful and already confirmed: repeated to a
//     fixed point) or from disassembled code in no function;
//   - referenced from data, or held by an aligned word of a data section (a vtable or RTTI slot, a function pointer);
//   - an export (an external entry point).
// One JSON line per function: addr, found, doubtful, confirmed, evidence (the kinds of reference that confirmed it).
//@category BFBC2.Decomp
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public class ConfirmFunctions extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        long junkLo = Long.parseLong(args[2], 16), junkHi = Long.parseLong(args[3], 16);
        Map<Long, String> found = new HashMap<>();
        Pattern p = Pattern.compile("\"addr\":(\\d+),\"found\":\"(\\w+)\"");
        if (Files.exists(Path.of(args[0])))
            for (String l : Files.readAllLines(Path.of(args[0]))) {
                Matcher m = p.matcher(l);
                if (m.find()) found.put(Long.parseLong(m.group(1)), m.group(2));
            }
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();
        ReferenceManager rm = currentProgram.getReferenceManager();
        SymbolTable st = currentProgram.getSymbolTable();
        Set<Long> doubtful = new HashSet<>();
        List<Function> all = new ArrayList<>();
        for (Function f : fm.getFunctions(true)) {
            all.add(f);
            long e = f.getEntryPoint().getOffset();
            if ("prologue".equals(found.get(e)) || (e >= junkLo && e <= junkHi)) doubtful.add(e);
        }
        Map<Long, Set<String>> evidence = new HashMap<>();
        // data words pointing at a doubtful entry
        for (MemoryBlock b : mem.getBlocks()) {
            if (b.isExecute() || !b.isInitialized() || b.getSize() > (64L << 20)) continue;
            byte[] buf = new byte[(int) b.getSize()];
            b.getBytes(b.getStart(), buf);
            long base = b.getStart().getOffset();
            for (int i = (int) ((4 - (base & 3)) & 3); i + 4 <= buf.length; i += 4) {
                long v = (buf[i] & 0xFFL) | (buf[i + 1] & 0xFFL) << 8 | (buf[i + 2] & 0xFFL) << 16 | (buf[i + 3] & 0xFFL) << 24;
                if (doubtful.contains(v)) evidence.computeIfAbsent(v, k -> new TreeSet<>()).add("data pointer");
            }
        }
        // exports and data references
        Map<Long, List<Long>> codeFrom = new HashMap<>();   // doubtful entry -> entries of the functions referencing it
        for (long e : doubtful) {
            Address a = toAddr(e);
            if (st.isExternalEntryPoint(a)) evidence.computeIfAbsent(e, k -> new TreeSet<>()).add("export");
            for (Reference r : rm.getReferencesTo(a)) {
                Address from = r.getFromAddress();
                if (!from.isMemoryAddress()) continue;
                MemoryBlock fb = mem.getBlock(from);
                if (fb != null && !fb.isExecute()) { evidence.computeIfAbsent(e, k -> new TreeSet<>()).add("data reference"); continue; }
                Function cf = fm.getFunctionContaining(from);
                if (cf == null) {
                    if (getInstructionAt(from) != null) evidence.computeIfAbsent(e, k -> new TreeSet<>()).add("code outside functions");
                    continue;
                }
                codeFrom.computeIfAbsent(e, k -> new ArrayList<>()).add(cf.getEntryPoint().getOffset());
            }
        }
        Set<Long> confirmed = new HashSet<>(evidence.keySet());
        boolean changed = true;
        while (changed) {
            changed = false;
            for (Map.Entry<Long, List<Long>> c : codeFrom.entrySet()) {
                if (confirmed.contains(c.getKey())) continue;
                for (long from : c.getValue()) {
                    if (from == c.getKey()) continue;                    // a function calling itself proves nothing
                    if (!doubtful.contains(from) || confirmed.contains(from)) {
                        confirmed.add(c.getKey());
                        evidence.computeIfAbsent(c.getKey(), k -> new TreeSet<>()).add(doubtful.contains(from) ? "call from a confirmed function" : "call or jump");
                        changed = true;
                        break;
                    }
                }
            }
        }
        long nd = 0, nc = 0;
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[1]), StandardCharsets.UTF_8))) {
            for (Function f : all) {
                long e = f.getEntryPoint().getOffset();
                String how = found.getOrDefault(e, "analysis");
                boolean d = doubtful.contains(e), c = !d || confirmed.contains(e);
                if (d) { nd++; if (c) nc++; }
                StringBuilder ev = new StringBuilder("[");
                for (String s : evidence.getOrDefault(e, Set.of())) ev.append(ev.length() > 1 ? "," : "").append('"').append(s).append('"');
                out.println("{\"addr\":" + e + ",\"found\":\"" + how + "\",\"doubtful\":" + d + ",\"confirmed\":" + c + ",\"evidence\":" + ev + "]}");
            }
        }
        println("ConfirmFunctions: " + all.size() + " functions, " + nd + " doubtful, " + nc + " of them confirmed");
    }
}

// ExportDb.java - decompile every function to pseudo-C and write the raw index and the coverage measurement (DC0).
// Args: <out folder> [threads=4, 0: write measure.json only] [timeout seconds=90] [first address hex] [last address hex]
//
// Writes, under the out folder (the private database; never inside a repo):
//   pseudo/<XXXX>.c        the pseudo-C of every function whose entry lies in 0xXXXX0000 - 0xXXXXFFFF, in address order
//   index/functions.jsonl  one line per function: entry, size, name, namespace, class?, signature, convention, callers,
//                          callees, strings referenced, the file and byte range of its pseudo-C, decompile status
//   index/measure.json     counts: functions, named, in a class, typed; analysis gaps (code bytes in no function,
//                          instructions in no function, computed jumps and calls without a resolved target, error
//                          bookmarks, decompile failures)
// Several decompiler processes run at once (threads); the program is only read.
//@category BFBC2.Decomp
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import ghidra.util.task.TaskMonitor;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;

public class ExportDb extends GhidraScript {
    static String js(String s) {
        if (s == null) return "null";
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c == '\n') b.append("\\n");
            else if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    record Res(Function f, String c, String status) {}

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        File outDir = new File(args[0]);
        int threads = args.length > 1 ? Integer.parseInt(args[1]) : 4;
        int timeout = args.length > 2 ? Integer.parseInt(args[2]) : 90;
        long first = args.length > 3 ? Long.parseLong(args[3], 16) : 0;
        long last = args.length > 4 ? Long.parseLong(args[4], 16) : 0xFFFFFFFFL;
        File pseudo = new File(outDir, "pseudo"), index = new File(outDir, "index");
        pseudo.mkdirs(); index.mkdirs();
        if (threads == 0) {                     // measure only (decompile counts left at 0)
            measure(new File(index, "measure.json"), 0, 0, 0);
            return;
        }
        FunctionManager fm = currentProgram.getFunctionManager();
        Listing listing = currentProgram.getListing();

        // functions grouped by 64 KiB range
        TreeMap<Long, List<Function>> groups = new TreeMap<>();
        for (Function f : fm.getFunctions(true)) {
            long e = f.getEntryPoint().getOffset();
            if (e < first || e > last) continue;
            groups.computeIfAbsent(e >>> 16, k -> new ArrayList<>()).add(f);
        }
        ExecutorService pool = Executors.newFixedThreadPool(threads);
        ThreadLocal<DecompInterface> tl = ThreadLocal.withInitial(() -> {
            DecompInterface d = new DecompInterface();
            DecompileOptions o = new DecompileOptions();
            d.setOptions(o);
            d.toggleCCode(true);
            d.toggleSyntaxTree(false);
            d.setSimplificationStyle("decompile");
            d.openProgram(currentProgram);
            return d;
        });
        List<DecompInterface> all = Collections.synchronizedList(new ArrayList<>());
        long done = 0, total = 0, ok = 0, failed = 0, timedOut = 0;
        for (List<Function> l : groups.values()) total += l.size();
        long t0 = System.currentTimeMillis();
        try (PrintWriter idx = new PrintWriter(new BufferedWriter(new OutputStreamWriter(
                new FileOutputStream(new File(index, first == 0 && last == 0xFFFFFFFFL ? "functions.jsonl"
                        : String.format("functions_%08x.jsonl", first))), StandardCharsets.UTF_8)))) {
            for (Map.Entry<Long, List<Function>> g : groups.entrySet()) {
                if (monitor.isCancelled()) break;
                String fname = String.format("%04X.c", g.getKey());
                List<Future<Res>> fut = new ArrayList<>();
                for (Function f : g.getValue()) {
                    fut.add(pool.submit(() -> {
                        DecompInterface d = tl.get();
                        if (!all.contains(d)) all.add(d);
                        if (f.isThunk() || f.isExternal()) return new Res(f, null, "thunk");
                        try {
                            DecompileResults r = d.decompileFunction(f, timeout, TaskMonitor.DUMMY);
                            if (r != null && r.decompileCompleted() && r.getDecompiledFunction() != null)
                                return new Res(f, r.getDecompiledFunction().getC(), "ok");
                            String m = r == null ? "null" : r.getErrorMessage();
                            return new Res(f, null, m != null && m.toLowerCase().contains("timeout") ? "timeout" : "failed: " + (m == null ? "" : m.trim().replace('\n', ' ')));
                        } catch (Exception x) { return new Res(f, null, "failed: " + x); }
                    }));
                }
                try (OutputStream os = new BufferedOutputStream(new FileOutputStream(new File(pseudo, fname)))) {
                    long pos = 0;
                    for (Future<Res> fr : fut) {
                        Res r = fr.get();
                        Function f = r.f();
                        String head = String.format("// ==== %08X %s\n", f.getEntryPoint().getOffset(), f.getName(true));
                        String body = r.c() != null ? r.c() : "// (no pseudo-C: " + r.status() + ")\n";
                        byte[] hb = head.getBytes(StandardCharsets.UTF_8), bb = (body + "\n").getBytes(StandardCharsets.UTF_8);
                        os.write(hb); os.write(bb);
                        long start = pos;
                        pos += hb.length + bb.length;
                        if (r.status().equals("ok")) ok++; else if (r.status().equals("timeout")) timedOut++; else if (!r.status().equals("thunk")) failed++;
                        idx.println(line(f, fname, start, pos - start, r.status()));
                        done++;
                    }
                }
                if ((g.getKey() & 0xF) == 0)
                    println(String.format("ExportDb: %d / %d functions, %ds", done, total, (System.currentTimeMillis() - t0) / 1000));
            }
        } finally {
            pool.shutdown();
            pool.awaitTermination(1, TimeUnit.MINUTES);
            for (DecompInterface d : all) d.dispose();
        }
        if (first == 0 && last == 0xFFFFFFFFL || args.length > 5) measure(new File(index, "measure.json"), ok, failed, timedOut);
        println(String.format("ExportDb: %d functions (%d ok, %d failed, %d timed out) in %ds", done, ok, failed, timedOut,
                (System.currentTimeMillis() - t0) / 1000));
    }

    String line(Function f, String file, long off, long len, String status) {
        StringBuilder b = new StringBuilder("{");
        b.append("\"addr\":").append(f.getEntryPoint().getOffset());
        b.append(",\"size\":").append(f.getBody().getNumAddresses());
        b.append(",\"end\":").append(f.getBody().getMaxAddress().getOffset());
        b.append(",\"name\":").append(js(f.getName()));
        Namespace ns = f.getParentNamespace();
        b.append(",\"namespace\":").append(js(ns.isGlobal() ? null : ns.getName(true)));
        b.append(",\"is_class\":").append(ns instanceof GhidraClass);
        b.append(",\"source\":").append(js(f.getSymbol().getSource().toString()));
        b.append(",\"signature\":").append(js(f.getPrototypeString(false, false)));
        b.append(",\"convention\":").append(js(f.getCallingConventionName()));
        b.append(",\"signature_source\":").append(js(f.getSignatureSource().toString()));
        String thisType = null;
        if (f.getParameterCount() > 0 && f.getParameter(0).getName().equals("this")) thisType = f.getParameter(0).getDataType().getPathName();
        b.append(",\"this_type\":").append(js(thisType));
        b.append(",\"thunk\":").append(f.isThunk());
        b.append(",\"callers\":[");
        boolean firstItem = true;
        for (Function c : f.getCallingFunctions(TaskMonitor.DUMMY)) { b.append(firstItem ? "" : ",").append(c.getEntryPoint().getOffset()); firstItem = false; }
        b.append("],\"callees\":[");
        firstItem = true;
        for (Function c : f.getCalledFunctions(TaskMonitor.DUMMY)) { b.append(firstItem ? "" : ",").append(c.getEntryPoint().getOffset()); firstItem = false; }
        b.append("],\"strings\":[");
        firstItem = true;
        Set<Address> seen = new HashSet<>();
        ReferenceManager rm = currentProgram.getReferenceManager();
        Listing listing = currentProgram.getListing();
        for (Address a : rm.getReferenceSourceIterator(f.getBody(), true)) {
            for (Reference r : rm.getReferencesFrom(a)) {
                Address to = r.getToAddress();
                if (!to.isMemoryAddress() || !seen.add(to)) continue;
                Data d = listing.getDataAt(to);
                if (d == null || !d.hasStringValue()) continue;
                Object v = d.getValue();
                String s = v == null ? "" : v.toString();
                if (s.length() > 200) s = s.substring(0, 200);
                b.append(firstItem ? "" : ",").append("[").append(to.getOffset()).append(",").append(js(s)).append("]");
                firstItem = false;
                if (seen.size() > 400) break;
            }
        }
        b.append("],\"file\":").append(js(file)).append(",\"offset\":").append(off).append(",\"length\":").append(len);
        b.append(",\"status\":").append(js(status)).append("}");
        return b.toString();
    }

    void measure(File out, long ok, long failed, long timedOut) throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        Listing listing = currentProgram.getListing();
        Memory mem = currentProgram.getMemory();
        long n = 0, named = 0, inClass = 0, typedThis = 0, sigSet = 0, thunks = 0, bytes = 0;
        AddressSet inFuncs = new AddressSet();
        for (Function f : fm.getFunctions(true)) {
            n++;
            if (f.isThunk()) thunks++;
            if (f.getSymbol().getSource() != SourceType.DEFAULT && !f.getName().startsWith("FUN_")) named++;
            if (f.getParentNamespace() instanceof GhidraClass) inClass++;
            if (f.getSignatureSource() != SourceType.DEFAULT) sigSet++;
            if (f.getParameterCount() > 0 && f.getParameter(0).getName().equals("this")) {
                DataType dt = f.getParameter(0).getDataType();
                if (dt instanceof Pointer p && p.getDataType() instanceof Structure s && !s.isNotYetDefined() && s.getNumDefinedComponents() > 0) typedThis++;
            }
            bytes += f.getBody().getNumAddresses();
            inFuncs.add(f.getBody());
        }
        // executable memory not in any function
        AddressSet exec = new AddressSet();
        for (MemoryBlock b : mem.getBlocks()) if (b.isExecute() && b.isInitialized()) exec.add(b.getStart(), b.getEnd());
        AddressSetView outside = exec.subtract(inFuncs);
        long instrOutside = 0, codeOutsideBytes = 0, padBytes = 0, undefinedBytes = 0, dataBytes = 0, regions = 0, bigRegions = 0;
        List<String> largest = new ArrayList<>();
        // every gap RCX0 has to know: index/gaps.jsonl (outside regions with code or 64+ undefined bytes, unresolved jumps)
        PrintWriter gaps = new PrintWriter(new BufferedWriter(new OutputStreamWriter(
                new FileOutputStream(new File(out.getParentFile(), "gaps.jsonl")), StandardCharsets.UTF_8)));
        TreeMap<Long, String> bySize = new TreeMap<>(Comparator.reverseOrder());
        for (AddressRange r : outside) {
            long len = r.getLength();
            regions++;
            long ins = 0, insBytes = 0;
            InstructionIterator it = listing.getInstructions(new AddressSet(r), true);
            while (it.hasNext()) { Instruction i = it.next(); ins++; insBytes += i.getLength(); }
            instrOutside += ins;
            codeOutsideBytes += insBytes;
            long data = 0;
            DataIterator di = listing.getDefinedData(new AddressSet(r), true);
            while (di.hasNext()) data += di.next().getLength();
            dataBytes += data;
            long rest = len - insBytes - data;
            // padding: bytes all 0xCC / 0x90 / 0x00
            boolean pad = true;
            if (rest > 0 && len <= 1 << 20) {
                byte[] buf = new byte[(int) len];
                try { mem.getBytes(r.getMinAddress(), buf); } catch (Exception x) { pad = false; }
                for (byte bt : buf) if (bt != (byte) 0xCC && bt != (byte) 0x90 && bt != 0) { pad = false; break; }
            } else if (rest > 0) pad = false;
            if (pad) padBytes += rest; else undefinedBytes += rest;
            if (ins > 0 || (!pad && rest >= 64))
                gaps.println("{\"kind\":\"region\",\"start\":" + r.getMinAddress().getOffset() + ",\"end\":" + r.getMaxAddress().getOffset()
                        + ",\"instructions\":" + ins + ",\"instruction_bytes\":" + insBytes + ",\"data_bytes\":" + data
                        + ",\"undefined_bytes\":" + (pad ? 0 : rest) + "}");
            if (!pad && rest >= 64) {
                bigRegions++;
                bySize.put(rest * 100000000L + (r.getMinAddress().getOffset() & 0xFFFFFFF), String.format("%08X-%08X (%d undefined bytes, %d instructions)", r.getMinAddress().getOffset(), r.getMaxAddress().getOffset(), rest, ins));
            }
        }
        int k = 0;
        for (String s : bySize.values()) { if (k++ >= 40) break; largest.add(s); }
        // computed jumps and calls
        long cj = 0, cjUnres = 0, cc = 0, ccUnres = 0, callsTotal = 0;
        InstructionIterator it = listing.getInstructions(true);
        while (it.hasNext()) {
            Instruction i = it.next();
            ghidra.program.model.symbol.FlowType ft = i.getFlowType();
            if (ft.isCall()) callsTotal++;
            if (!ft.isComputed()) continue;
            boolean resolved = false;
            for (Reference r : i.getReferencesFrom()) if (r.getReferenceType().isFlow() || r.getReferenceType().isCall() || r.getReferenceType().isJump()) { resolved = true; break; }
            if (ft.isJump()) {
                cj++;
                if (!resolved) {
                    cjUnres++;
                    Function cf = fm.getFunctionContaining(i.getAddress());
                    gaps.println("{\"kind\":\"jump\",\"addr\":" + i.getAddress().getOffset() + ",\"function\":"
                            + (cf == null ? "null" : cf.getEntryPoint().getOffset()) + ",\"text\":" + js(i.toString()) + "}");
                }
            }
            else if (ft.isCall()) { cc++; if (!resolved) ccUnres++; }
        }
        gaps.close();
        long errBookmarks = currentProgram.getBookmarkManager().getBookmarkCount("Error");
        long warnBookmarks = currentProgram.getBookmarkManager().getBookmarkCount("Warning");
        try (PrintWriter o = new PrintWriter(new OutputStreamWriter(new FileOutputStream(out), StandardCharsets.UTF_8))) {
            o.println("{");
            o.println("  \"functions\": " + n + ",");
            o.println("  \"thunks\": " + thunks + ",");
            o.println("  \"named\": " + named + ",");
            o.println("  \"in_class\": " + inClass + ",");
            o.println("  \"this_typed_with_layout\": " + typedThis + ",");
            o.println("  \"signature_not_default\": " + sigSet + ",");
            o.println("  \"bytes_in_functions\": " + bytes + ",");
            o.println("  \"executable_bytes\": " + exec.getNumAddresses() + ",");
            o.println("  \"outside_regions\": " + regions + ",");
            o.println("  \"outside_instructions\": " + instrOutside + ",");
            o.println("  \"outside_instruction_bytes\": " + codeOutsideBytes + ",");
            o.println("  \"outside_data_bytes\": " + dataBytes + ",");
            o.println("  \"outside_padding_bytes\": " + padBytes + ",");
            o.println("  \"outside_undefined_bytes\": " + undefinedBytes + ",");
            o.println("  \"outside_undefined_regions_64_plus\": " + bigRegions + ",");
            o.println("  \"largest_undefined_regions\": [" + String.join(",", largest.stream().map(ExportDb::js).toList()) + "],");
            o.println("  \"calls\": " + callsTotal + ",");
            o.println("  \"computed_jumps\": " + cj + ",");
            o.println("  \"computed_jumps_unresolved\": " + cjUnres + ",");
            o.println("  \"computed_calls\": " + cc + ",");
            o.println("  \"computed_calls_unresolved\": " + ccUnres + ",");
            o.println("  \"error_bookmarks\": " + errBookmarks + ",");
            o.println("  \"warning_bookmarks\": " + warnBookmarks + ",");
            o.println("  \"decompiled_ok\": " + ok + ",");
            o.println("  \"decompile_failed\": " + failed + ",");
            o.println("  \"decompile_timed_out\": " + timedOut);
            o.println("}");
        }
    }
}

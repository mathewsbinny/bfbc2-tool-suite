// DiscoverFunctions.java - a conservative function-discovery pass over what auto-analysis left outside functions (DC0).
// Args: <report.json> <discovered.jsonl> [skip first hex] [skip last hex]   (.text, without the skipped range)
//
// Two kinds of gap inside executable memory, outside every function:
//  1. Disassembled code in no function (code Ghidra reached through a reference, a jump table or an exception
//     handler's table but never made a function): a function is made at each such region's first instruction when
//     something references it or it starts on a 16-byte boundary.
//  2. Bytes never disassembled: at each 16-byte boundary right after 0xCC / 0x90 padding or a return, whose bytes start
//     like an MSVC x86 prologue (push ebp; mov ebp, esp / sub esp / push -1; push handler / push esi, edi, ebx / mov
//     eax|ecx, [esp+n] ...), the code is disassembled and a function made, kept only when its flow ends cleanly (no
//     bad instruction, no error bookmark).
// Writes counts to the report and each made function with how it was found ("orphan" for 1, "prologue" for 2) to
// discovered.jsonl, which ConfirmFunctions.java reads. Run before ExportDb.java; the project is saved.
//@category BFBC2.Decomp
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

public class DiscoverFunctions extends GhidraScript {
    static boolean prologue(byte[] b) {
        int b0 = b[0] & 0xFF, b1 = b[1] & 0xFF, b2 = b[2] & 0xFF;
        if (b0 == 0x55 && b1 == 0x8B && b2 == 0xEC) return true;                 // push ebp; mov ebp, esp
        if (b0 == 0x83 && b1 == 0xEC) return true;                              // sub esp, imm8
        if (b0 == 0x81 && b1 == 0xEC) return true;                              // sub esp, imm32
        if (b0 == 0x6A && b1 == 0xFF && b2 == 0x68) return true;                // push -1; push handler (SEH frame)
        if (b0 == 0x64 && b1 == 0xA1) return true;                              // mov eax, fs:[0]
        if (b0 == 0x8B && b1 == 0x44 && b2 == 0x24) return true;                // mov eax, [esp+n]
        if (b0 == 0x8B && b1 == 0x4C && b2 == 0x24) return true;                // mov ecx, [esp+n]
        if (b0 == 0x8B && b1 == 0x54 && b2 == 0x24) return true;                // mov edx, [esp+n]
        if ((b0 == 0x56 || b0 == 0x57 || b0 == 0x53) && (b1 == 0x8B || b1 == 0x56 || b1 == 0x57 || b1 == 0x53)) return true;
        if (b0 == 0x51 && (b1 == 0x56 || b1 == 0x53 || b1 == 0x55 || b1 == 0x8B)) return true;  // push ecx (a local)
        return false;
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        long skipLo = args.length > 3 ? Long.parseLong(args[2], 16) : 0, skipHi = args.length > 3 ? Long.parseLong(args[3], 16) : 0;
        PrintWriter disc = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[1]), StandardCharsets.UTF_8));
        FunctionManager fm = currentProgram.getFunctionManager();
        Listing listing = currentProgram.getListing();
        Memory mem = currentProgram.getMemory();
        ReferenceManager rm = currentProgram.getReferenceManager();
        MemoryBlock text = mem.getBlock(".text");
        AddressSet inFuncs = new AddressSet();
        for (Function f : fm.getFunctions(true)) inFuncs.add(f.getBody());
        AddressSet scope = new AddressSet(text.getStart(), text.getEnd());
        if (skipHi > skipLo) scope.delete(toAddr(skipLo), toAddr(skipHi));
        AddressSetView outside = scope.subtract(inFuncs);
        int fromCode = 0, fromBytes = 0, tried = 0, rejected = 0;
        List<Address> codeStarts = new ArrayList<>(), byteStarts = new ArrayList<>();
        for (AddressRange r : outside) {
            Instruction first = listing.getInstructionAt(r.getMinAddress());
            if (first == null) {
                InstructionIterator it = listing.getInstructions(new AddressSet(r), true);
                if (it.hasNext()) first = it.next();
            }
            if (first != null) {
                Address a = first.getAddress();
                if (rm.hasReferencesTo(a) || (a.getOffset() & 0xF) == 0) codeStarts.add(a);
                continue;
            }
            // never disassembled: 16-byte boundaries after padding or a return
            long lo = r.getMinAddress().getOffset(), hi = r.getMaxAddress().getOffset();
            for (long a = (lo + 15) & ~15L; a + 8 <= hi; a += 16) {
                byte[] b = new byte[3];
                byte prev;
                try { mem.getBytes(toAddr(a), b); prev = mem.getByte(toAddr(a - 1)); } catch (Exception x) { continue; }
                int p = prev & 0xFF;
                if (!(p == 0xCC || p == 0x90 || p == 0xC3 || p == 0xC2 || p == 0x00)) continue;
                if (prologue(b)) byteStarts.add(toAddr(a));
            }
        }
        for (Address a : codeStarts) {
            if (monitor.isCancelled()) break;
            if (fm.getFunctionContaining(a) != null) continue;
            if (new CreateFunctionCmd(a).applyTo(currentProgram, monitor)) { fromCode++; disc.println("{\"addr\":" + a.getOffset() + ",\"found\":\"orphan\"}"); }
        }
        BookmarkManager bm = currentProgram.getBookmarkManager();
        for (Address a : byteStarts) {
            if (monitor.isCancelled()) break;
            if (fm.getFunctionContaining(a) != null || listing.getInstructionAt(a) != null) continue;
            tried++;
            int errBefore = bm.getBookmarkCount("Error");
            DisassembleCommand dc = new DisassembleCommand(a, null, true);
            dc.applyTo(currentProgram, monitor);
            AddressSetView got = dc.getDisassembledAddressSet();
            boolean bad = got == null || got.isEmpty() || bm.getBookmarkCount("Error") > errBefore || !got.intersect(inFuncs).isEmpty();
            if (!bad && new CreateFunctionCmd(a).applyTo(currentProgram, monitor)) { fromBytes++; inFuncs.add(fm.getFunctionAt(a).getBody()); disc.println("{\"addr\":" + a.getOffset() + ",\"found\":\"prologue\"}"); continue; }
            rejected++;
            if (got != null) for (AddressRange gr : got) listing.clearCodeUnits(gr.getMinAddress(), gr.getMaxAddress(), false);
        }
        disc.close();
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            out.println("{\"code_regions_seen\": " + codeStarts.size() + ", \"functions_from_code\": " + fromCode
                    + ", \"prologue_candidates\": " + byteStarts.size() + ", \"tried\": " + tried
                    + ", \"functions_from_bytes\": " + fromBytes + ", \"rejected\": " + rejected + "}");
        }
        println("DiscoverFunctions: from code " + fromCode + " of " + codeStarts.size() + ", from bytes " + fromBytes
                + " of " + tried + " tried (" + rejected + " rejected)");
    }
}

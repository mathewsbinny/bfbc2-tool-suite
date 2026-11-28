// ApplyNames.java - apply the spec's and Project OPFOR's named addresses as names and comments (round DC0).
// Args: <names.json from tools/decomp/extract_names.py> <report.jsonl>
//
// For every address: a declared name (quality 2 - 3) renames a function whose entry it is, when the function still has
// Ghidra's default name (FUN_...); `A::b` puts it in namespace A. A name picked from a sentence (quality 1) becomes
// `spec_<words>`. Any other address (inside a function, data) gets the name as a label. Every address gets a comment
// citing its rule ids and sources (plate comment on a function entry, pre comment elsewhere). The report says, per
// address, where it landed: a function entry, inside a function, inside an instruction (the spec's address may be
// wrong), data, or nothing defined.
//@category BFBC2.Decomp
import com.google.gson.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.util.NamespaceUtils;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class ApplyNames extends GhidraScript {
    static String js(String s) {
        if (s == null) return "null";
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    static String clean(String n) {
        return n.replaceAll("[^A-Za-z0-9_:<>~]", "_");
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        JsonObject root = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();
        Listing listing = currentProgram.getListing();
        FunctionManager fm = currentProgram.getFunctionManager();
        SymbolTable st = currentProgram.getSymbolTable();
        Map<String, Integer> counts = new TreeMap<>();
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[1]), StandardCharsets.UTF_8))) {
            for (JsonElement je : root.getAsJsonArray("entries")) {
                JsonObject e = je.getAsJsonObject();
                Address a = toAddr(e.get("addr").getAsLong());
                MemoryBlock blk = currentProgram.getMemory().getBlock(a);
                String name = e.get("name").isJsonNull() ? null : clean(e.get("name").getAsString());
                int q = e.get("name_quality").getAsInt();
                if (name != null && q < 2) name = "spec_" + name.replace("::", "_");
                List<String> rules = new ArrayList<>();
                for (JsonElement r : e.getAsJsonArray("rules")) rules.add(r.getAsString());
                List<String> srcs = new ArrayList<>();
                for (JsonElement r : e.getAsJsonArray("sources")) srcs.add(r.getAsString());
                StringBuilder cmt = new StringBuilder("[DC0] ");
                cmt.append(rules.isEmpty() ? "no rule" : String.join(", ", rules)).append(" (").append(String.join(", ", srcs)).append(")");
                JsonArray cites = e.getAsJsonArray("cites");
                for (int i = 0; i < Math.min(3, cites.size()); i++) {
                    JsonObject c = cites.get(i).getAsJsonObject();
                    cmt.append("\n  ").append(c.get("where").getAsString()).append(": ").append(c.get("context").getAsString());
                }
                String where;
                String applied = null;
                Function f = fm.getFunctionAt(a);
                CodeUnit cu = listing.getCodeUnitContaining(a);
                if (blk == null) where = "unmapped";
                else if (f != null) {
                    where = "function_entry";
                    if (name != null) {
                        boolean isDefault = f.getSymbol().getSource() == SourceType.DEFAULT || f.getName().startsWith("FUN_");
                        try {
                            if (isDefault) {
                                String simple = name;
                                if (name.contains("::")) {
                                    int k = name.lastIndexOf("::");
                                    Namespace ns = NamespaceUtils.createNamespaceHierarchy(name.substring(0, k), null, currentProgram, SourceType.USER_DEFINED);
                                    if (f.getParentNamespace().isGlobal()) f.setParentNamespace(ns);
                                    simple = name.substring(k + 2);
                                }
                                f.setName(simple, SourceType.USER_DEFINED);
                                applied = "function";
                            } else {
                                st.createLabel(a, name.replace("::", "_"), SourceType.USER_DEFINED);
                                applied = "label (function already named " + f.getName(true) + ")";
                            }
                        } catch (Exception x) { applied = "failed: " + x.getMessage(); }
                    }
                    String old = f.getComment();
                    if (old == null || !old.contains("[DC0]")) f.setComment(old == null ? cmt.toString() : old + "\n" + cmt);
                } else {
                    Function in = fm.getFunctionContaining(a);
                    if (cu instanceof Instruction && !cu.getMinAddress().equals(a)) where = "mid_instruction";
                    else if (in != null) where = "in_function";
                    else if (cu instanceof Instruction) where = "code_no_function";
                    else if (cu instanceof Data d && d.isDefined()) where = "data";
                    else where = blk.isExecute() ? "undefined_in_code" : "undefined_data";
                    if (name != null && !where.equals("mid_instruction")) {
                        try { st.createLabel(a, name.replace("::", "_"), SourceType.USER_DEFINED); applied = "label"; }
                        catch (Exception x) { applied = "failed: " + x.getMessage(); }
                    }
                    String old = listing.getComment(CommentType.PRE, a);
                    if (old == null || !old.contains("[DC0]"))
                        listing.setComment(a, CommentType.PRE, old == null ? cmt.toString() : old + "\n" + cmt);
                }
                counts.merge(where, 1, Integer::sum);
                if (applied != null) counts.merge("applied " + applied.split(" ")[0], 1, Integer::sum);
                Function cont = fm.getFunctionContaining(a);
                out.println("{\"addr\":" + a.getOffset() + ",\"where\":" + js(where) + ",\"name\":" + js(name)
                        + ",\"applied\":" + js(applied) + ",\"function\":" + (cont == null ? "null" : cont.getEntryPoint().getOffset())
                        + ",\"block\":" + js(blk == null ? null : blk.getName()) + ",\"rules\":" + js(String.join(",", rules)) + "}");
            }
        }
        println("ApplyNames: " + counts);
    }
}

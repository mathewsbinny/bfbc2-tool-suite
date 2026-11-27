// ExportSymbols.java - write every user-named symbol of the current program as JSON lines.
// Args: outFile. One line per symbol whose source is USER_DEFINED or IMPORTED (labels, functions, namespaces'
// members), with its address, full name, kind and comment if any. Used by tools/decomp to bring in the names of
// an older Ghidra project (Project OPFOR BFBC2's research\ghidra_project). Read only: changes nothing.
//@category BFBC2.Decomp
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;

public class ExportSymbols extends GhidraScript {
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

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        int n = 0;
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            SymbolTable st = currentProgram.getSymbolTable();
            Listing listing = currentProgram.getListing();
            for (Symbol s : st.getAllSymbols(true)) {
                SourceType src = s.getSource();
                if (src != SourceType.USER_DEFINED && src != SourceType.IMPORTED) continue;
                if (!s.getAddress().isMemoryAddress()) continue;
                String kind = s.getSymbolType().toString();
                String cmt = null;
                Function f = s.getSymbolType() == SymbolType.FUNCTION ? (Function) s.getObject() : null;
                if (f != null) cmt = f.getComment();
                if (cmt == null) cmt = listing.getComment(CommentType.PLATE, s.getAddress());
                if (cmt == null) cmt = listing.getComment(CommentType.EOL, s.getAddress());
                out.println("{\"addr\":" + s.getAddress().getOffset() + ",\"name\":" + js(s.getName(true))
                        + ",\"kind\":" + js(kind) + ",\"source\":" + js(src.toString()) + ",\"comment\":" + js(cmt) + "}");
                n++;
            }
        }
        println("ExportSymbols: " + n + " symbols to " + args[0]);
    }
}

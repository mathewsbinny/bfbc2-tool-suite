// DbxEdit - edits BFBC2 binary .dbx ("{binary}" Frostbite 1 XML) in place of the binary tree, so files that
// are not changed are written back byte for byte (roundtrip mode checks that).
//
// Binary layout (as read by DbxTool): "{binary}" | u32 BE payload start | u32 BE (kept) | u32 BE payload
// start - 24 | u32 BE string count | count x u32 BE string offsets | NUL-terminated UTF-8 strings | payload.
// Payload: elements written as 7-bit varint prefix (string index, 0 = close the open element), one byte
// (type << 4 | attribute count), attribute pairs (varint name index, varint value index), then by type:
// 0xA container (one 0 byte, children, close), 2 string (varint index), 7 array of numbers (varint count,
// varint size, raw big-endian values), else 2 raw bytes (bool / byte).
//
// usage: DbxEdit roundtrip <file.dbx|folder>             parse + write every file, report differences
//        DbxEdit edit <in.dbx> <out.dbx> <command> ...      commands (applied in order):
//            replace=<old>|<new>     every string exactly <old> becomes <new> (re-points references)
//            drop=<guid>             removes the instance with that guid; array items referring to it are
//                                    removed, single references to it become "null"
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

class Node
{
    public int Prefix;           // string index
    public int Type;             // high nibble of the type byte
    public List<int[]> Attrs = new List<int[]>();
    public byte[] Raw;           // value bytes for non-container types (after the attributes)
    public byte Null;            // container: the byte after the attributes
    public List<Node> Children;  // container children
}

static class DbxEdit
{
    static byte[] B;
    static int P;
    static List<string> Strings;
    static List<int> Offsets;
    static byte[] Blob;          // original string area (kept verbatim when no string changes)
    static uint HeaderKept;
    static byte[] Tail;          // bytes after the root element
    static bool StringsChanged;

    static uint BE(int o) { return (uint)(B[o] << 24 | B[o + 1] << 16 | B[o + 2] << 8 | B[o + 3]); }

    static int R128()
    {
        int result = 0, shift = 0;
        while (true)
        {
            byte b = B[P++];
            result |= (b & 127) << shift;
            shift += 7;
            if ((b >> 7) == 0) return result;
        }
    }

    static void W128(List<byte> o, int v)
    {
        do
        {
            byte b = (byte)(v & 127);
            v >>= 7;
            if (v != 0) b |= 128;
            o.Add(b);
        } while (v != 0);
    }

    static Node ReadNode(int prefix)
    {
        var n = new Node { Prefix = prefix };
        int t = B[P++];
        n.Type = t >> 4;
        int nattr = t & 15;
        for (int i = 0; i < nattr; i++) { int k = R128(); int v = R128(); n.Attrs.Add(new[] { k, v }); }
        int start = P;
        if (n.Type == 0xA)
        {
            n.Null = B[P++];
            n.Children = new List<Node>();
            while (true)
            {
                int pre = R128();
                if (pre == 0) break;
                n.Children.Add(ReadNode(pre));
            }
            return n;
        }
        if (n.Type == 2) R128();
        else if (n.Type == 7) { int cnt = R128(); int len = R128(); P += cnt * len; }
        else P += 2;
        n.Raw = new byte[P - start];
        Array.Copy(B, start, n.Raw, 0, n.Raw.Length);
        return n;
    }

    static List<Node> Load(string path, out bool ok)
    {
        B = File.ReadAllBytes(path);
        ok = B.Length > 24 && Encoding.ASCII.GetString(B, 0, 8) == "{binary}";
        if (!ok) return null;
        uint reloffset = BE(16), count = BE(20);
        HeaderKept = BE(12);
        Offsets = new List<int>();
        Strings = new List<string>();
        int strBase = 24 + 4 * (int)count;
        int payload = 24 + (int)reloffset;
        for (int i = 0; i < count; i++) Offsets.Add((int)BE(24 + 4 * i));
        foreach (int off in Offsets)
        {
            int s = strBase + off, e = s;
            while (B[e] != 0) e++;
            Strings.Add(Encoding.UTF8.GetString(B, s, e - s));
        }
        Blob = new byte[payload - strBase];
        Array.Copy(B, strBase, Blob, 0, Blob.Length);
        StringsChanged = false;
        P = payload;
        var roots = new List<Node>();
        while (P < B.Length)
        {
            int pre = R128();
            if (pre == 0) { P--; break; }
            roots.Add(ReadNode(pre));
        }
        Tail = new byte[B.Length - P];
        Array.Copy(B, P, Tail, 0, Tail.Length);
        return roots;
    }

    static void WriteNode(List<byte> o, Node n)
    {
        W128(o, n.Prefix);
        o.Add((byte)(n.Type << 4 | n.Attrs.Count));
        foreach (var a in n.Attrs) { W128(o, a[0]); W128(o, a[1]); }
        if (n.Type == 0xA)
        {
            o.Add(n.Null);
            foreach (var c in n.Children) WriteNode(o, c);
            o.Add(0);
        }
        else o.AddRange(n.Raw);
    }

    static byte[] Save(List<Node> roots)
    {
        byte[] blob = Blob;
        var offs = Offsets;
        if (StringsChanged)
        {
            var b = new List<byte>();
            offs = new List<int>();
            foreach (var s in Strings) { offs.Add(b.Count); b.AddRange(Encoding.UTF8.GetBytes(s)); b.Add(0); }
            blob = b.ToArray();
        }
        var payload = new List<byte>();
        foreach (var r in roots) WriteNode(payload, r);
        payload.AddRange(Tail);
        int reloffset = 4 * Strings.Count + blob.Length;
        var o = new List<byte>(Encoding.ASCII.GetBytes("{binary}"));
        Action<uint> be = v => { o.Add((byte)(v >> 24)); o.Add((byte)(v >> 16)); o.Add((byte)(v >> 8)); o.Add((byte)v); };
        be((uint)(24 + reloffset));
        be(HeaderKept);
        be((uint)reloffset);
        be((uint)Strings.Count);
        foreach (int off in offs) be((uint)off);
        o.AddRange(blob);
        o.AddRange(payload);
        return o.ToArray();
    }

    static int Str(string s)
    {
        int i = Strings.IndexOf(s);
        if (i >= 0) return i;
        Strings.Add(s);
        StringsChanged = true;
        return Strings.Count - 1;
    }

    static string Attr(Node n, string name)
    {
        foreach (var a in n.Attrs) if (Strings[a[0]] == name) return Strings[a[1]];
        return null;
    }

    static int Replace(string from, string to)
    {
        int hits = 0;
        for (int i = 0; i < Strings.Count; i++)
            if (Strings[i] == from) { Strings[i] = to; hits++; StringsChanged = true; }
        return hits;
    }

    // Removes instance <guid>; array items referring to it go, other references become "null".
    static int Drop(List<Node> nodes, string guid, ref int refs)
    {
        int removed = 0;
        for (int i = nodes.Count - 1; i >= 0; i--)
        {
            var n = nodes[i];
            string pre = Strings[n.Prefix];
            if (pre == "instance" && Attr(n, "guid") == guid) { nodes.RemoveAt(i); removed++; continue; }
            string r = Attr(n, "ref");
            if (r != null && (r == guid || r.EndsWith("/" + guid)))
            {
                refs++;
                if (pre == "item") { nodes.RemoveAt(i); continue; }
                foreach (var a in n.Attrs) if (Strings[a[0]] == "ref") a[1] = Str("null");
            }
            if (n.Children != null) removed += Drop(n.Children, guid, ref refs);
        }
        return removed;
    }

    static int Main(string[] args)
    {
        if (args.Length >= 2 && args[0] == "roundtrip")
        {
            var files = Directory.Exists(args[1]) ? Directory.GetFiles(args[1], "*.dbx", SearchOption.AllDirectories) : new[] { args[1] };
            int same = 0, diff = 0, skipped = 0;
            foreach (var f in files)
            {
                bool ok;
                List<Node> roots;
                try { roots = Load(f, out ok); } catch (Exception e) { Console.WriteLine("ERROR " + f + ": " + e.Message); diff++; continue; }
                if (!ok) { skipped++; continue; }
                var orig = B;
                var outb = Save(roots);
                if (outb.SequenceEqual(orig)) same++;
                else { diff++; if (diff <= 10) Console.WriteLine("DIFF " + f + " (" + orig.Length + " -> " + outb.Length + ")"); }
            }
            Console.WriteLine("roundtrip: " + same + " identical, " + diff + " different, " + skipped + " not binary");
            return diff == 0 ? 0 : 1;
        }
        if (args.Length >= 3 && args[0] == "edit")
        {
            bool ok;
            var roots = Load(args[1], out ok);
            if (!ok) { Console.Error.WriteLine("not a binary dbx: " + args[1]); return 2; }
            for (int i = 3; i < args.Length; i++)
            {
                string c = args[i];
                if (c.StartsWith("replace="))
                {
                    var p = c.Substring(8).Split('|');
                    int h = Replace(p[0], p[1]);
                    Console.WriteLine("replace " + p[0] + " -> " + p[1] + ": " + h + " string(s)");
                    if (h == 0) { Console.Error.WriteLine("not found: " + p[0]); return 1; }
                }
                else if (c.StartsWith("drop="))
                {
                    int refs = 0;
                    int n = Drop(roots, c.Substring(5), ref refs);
                    Console.WriteLine("drop " + c.Substring(5) + ": " + n + " instance(s), " + refs + " reference(s)");
                    if (n == 0) { Console.Error.WriteLine("no instance " + c.Substring(5)); return 1; }
                }
                else { Console.Error.WriteLine("unknown command " + c); return 2; }
            }
            File.WriteAllBytes(args[2], Save(roots));
            return 0;
        }
        Console.Error.WriteLine("usage: DbxEdit roundtrip <file|folder> | edit <in.dbx> <out.dbx> replace=<old>|<new> drop=<guid> ...");
        return 2;
    }
}

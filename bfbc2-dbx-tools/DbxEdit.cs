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
//            replaceprefix=<old>|<new>[|<except>...]  strings starting with <old> get <new> as prefix
//            partguid=<guid>         sets the partition's own guid (a copy under a new name gets a new one)
//            copyfield=<src.dbx>|<src guid>|<field>|<guid>  copies a field (with arrays) from another file's instance
//            set=<field>|<true|false|0-255>  sets a bool / byte field
//            dropfield=<name>        removes every field named <name> (fields the PC build does not have)
//            graft=<src.dbx>|<guid>[|<before guid>]  copies (inserted before that instance if given) that instance of another file, with every instance of that file it
//                                    references in-partition (members), strings remapped
//        a command argument "@file" reads commands from a file (one per line, # comments)
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

class Doc
{
    byte[] B;
    int P;
    public List<string> Strings;
    List<int> Offsets;
    byte[] Blob;          // original string area (kept verbatim when no string changes)
    uint HeaderKept;
    byte[] Tail;          // bytes after the root element
    public bool StringsChanged;
    public List<Node> Roots;
    public bool Ok;
    public byte[] Original { get { return B; } }

    uint BE(int o) { return (uint)(B[o] << 24 | B[o + 1] << 16 | B[o + 2] << 8 | B[o + 3]); }

    int R128()
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

    Node ReadNode(int prefix)
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

    public Doc(string path)
    {
        B = File.ReadAllBytes(path);
        Ok = B.Length > 24 && Encoding.ASCII.GetString(B, 0, 8) == "{binary}";
        if (!Ok) return;
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
        P = payload;
        Roots = new List<Node>();
        while (P < B.Length)
        {
            int pre = R128();
            if (pre == 0) { P--; break; }
            Roots.Add(ReadNode(pre));
        }
        Tail = new byte[B.Length - P];
        Array.Copy(B, P, Tail, 0, Tail.Length);
    }

    void WriteNode(List<byte> o, Node n)
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

    public byte[] Save()
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
        foreach (var r in Roots) WriteNode(payload, r);
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

    public int Str(string s)
    {
        int i = Strings.IndexOf(s);
        if (i >= 0) return i;
        Strings.Add(s);
        StringsChanged = true;
        return Strings.Count - 1;
    }

    public string Attr(Node n, string name)
    {
        foreach (var a in n.Attrs) if (Strings[a[0]] == name) return Strings[a[1]];
        return null;
    }

    public int Replace(string from, string to)
    {
        int hits = 0;
        for (int i = 0; i < Strings.Count; i++)
            if (Strings[i] == from) { Strings[i] = to; hits++; StringsChanged = true; }
        return hits;
    }

    // Every string starting with <from> (case-insensitive) gets <to> as its prefix, except strings starting with
    // one of <except>; the replacement keeps the case style of the first letter of <from> as found.
    public int ReplacePrefix(string from, string to, string[] except)
    {
        int hits = 0;
        for (int i = 0; i < Strings.Count; i++)
        {
            string s = Strings[i];
            if (!s.StartsWith(from, StringComparison.OrdinalIgnoreCase)) continue;
            if (except.Any(e => e.Length > 0 && s.StartsWith(e, StringComparison.OrdinalIgnoreCase))) continue;
            bool lower = s.Substring(0, from.Length) == s.Substring(0, from.Length).ToLowerInvariant();
            Strings[i] = (lower ? to.ToLowerInvariant() : to) + s.Substring(from.Length);
            hits++;
            StringsChanged = true;
        }
        return hits;
    }

    static bool SameGuid(string a, string b) { return string.Equals(a, b, StringComparison.OrdinalIgnoreCase); }

    // Removes instance <guid>; array items referring to it go, other references become "null".
    public int Drop(List<Node> nodes, string guid, ref int refs)
    {
        int removed = 0;
        for (int i = nodes.Count - 1; i >= 0; i--)
        {
            var n = nodes[i];
            string pre = Strings[n.Prefix];
            if (pre == "instance" && SameGuid(Attr(n, "guid"), guid)) { nodes.RemoveAt(i); removed++; continue; }
            string r = Attr(n, "ref");
            if (r != null && (SameGuid(r, guid) || r.EndsWith("/" + guid, StringComparison.OrdinalIgnoreCase)))
            {
                refs++;
                if (pre == "item") { nodes.RemoveAt(i); continue; }
                foreach (var a in n.Attrs) if (Strings[a[0]] == "ref") a[1] = Str("null");
            }
            if (n.Children != null) removed += Drop(n.Children, guid, ref refs);
        }
        return removed;
    }

    // Removes every field / array / complex element named <name> (a field the target build does not have).
    public int DropField(List<Node> nodes, string name)
    {
        int removed = 0;
        for (int i = nodes.Count - 1; i >= 0; i--)
        {
            var n = nodes[i];
            if (Strings[n.Prefix] != "instance" && Attr(n, "name") == name) { nodes.RemoveAt(i); removed++; continue; }
            if (n.Children != null) removed += DropField(n.Children, name);
        }
        return removed;
    }

    public int SetByte(List<Node> nodes, string name, byte v)
    {
        int n = 0;
        foreach (var x in nodes)
        {
            if (x.Children != null) n += SetByte(x.Children, name, v);
            else if (x.Type != 2 && x.Type != 7 && x.Raw != null && x.Raw.Length == 2 && Attr(x, "name") == name) { x.Raw = new byte[] { x.Raw[0], v }; n++; }
        }
        return n;
    }

    // Replaces (or adds) field <field> of this document's instance <guid> with the same field of src's instance
    // <srcGuid> (strings remapped). Returns false if either is missing.
    public bool CopyField(Doc src, string srcGuid, string field, string guid)
    {
        var si = src.Instances().FirstOrDefault(n => SameGuid(src.Attr(n, "guid"), srcGuid));
        var di = Instances().FirstOrDefault(n => SameGuid(Attr(n, "guid"), guid));
        if (si == null || di == null) return false;
        var sf = si.Children.FirstOrDefault(n => src.Attr(n, "name") == field);
        if (sf == null) return false;
        var copy = CopyFrom(src, sf);
        int at = di.Children.FindIndex(n => Attr(n, "name") == field);
        if (at >= 0) di.Children[at] = copy; else di.Children.Add(copy);
        return true;
    }

    public List<Node> Instances() { return Roots[0].Children.Where(c => Strings[c.Prefix] == "instance").ToList(); }

    // bare (same-partition) references inside a node tree
    public void LocalRefs(Node n, HashSet<string> outRefs)
    {
        string r = Attr(n, "ref");
        if (r != null && r.Length == 36 && r.IndexOf('/') < 0) outRefs.Add(r.ToUpperInvariant());
        if (n.Children != null) foreach (var c in n.Children) LocalRefs(c, outRefs);
    }

    Node CopyFrom(Doc src, Node n)
    {
        var c = new Node { Prefix = Str(src.Strings[n.Prefix]), Type = n.Type, Null = n.Null };
        foreach (var a in n.Attrs) c.Attrs.Add(new[] { Str(src.Strings[a[0]]), Str(src.Strings[a[1]]) });
        if (n.Type == 2)
        {
            // value = varint string index in the source document
            int idx = 0, shift = 0, k = 0;
            while (true) { byte b = n.Raw[k++]; idx |= (b & 127) << shift; shift += 7; if ((b >> 7) == 0) break; }
            var o = new List<byte>();
            W128(o, Str(src.Strings[idx]));
            c.Raw = o.ToArray();
        }
        else c.Raw = n.Raw;
        if (n.Children != null) { c.Children = new List<Node>(); foreach (var ch in n.Children) c.Children.Add(CopyFrom(src, ch)); }
        return c;
    }

    // Copies instance <guid> of src and every src instance it references in-partition (transitively) that this
    // document does not have yet; appended after the last instance.
    public int Graft(Doc src, string guid) { return Graft(src, guid, null); }

    // As Graft; the copied instances go before instance <before> (keeps the world objects' position in the file).
    public int Graft(Doc src, string guid, string before)
    {
        int at = Roots[0].Children.Count;
        if (before != null)
        {
            int i = Roots[0].Children.FindIndex(n => Strings[n.Prefix] == "instance" && SameGuid(Attr(n, "guid"), before));
            if (i >= 0) at = i;
        }
        var byGuid = new Dictionary<string, Node>();
        foreach (var n in src.Instances()) byGuid[src.Attr(n, "guid").ToUpperInvariant()] = n;
        var have = new HashSet<string>(Instances().Select(n => Attr(n, "guid").ToUpperInvariant()));
        var todo = new Stack<string>();
        todo.Push(guid.ToUpperInvariant());
        int added = 0;
        while (todo.Count > 0)
        {
            string g = todo.Pop();
            if (have.Contains(g) || !byGuid.ContainsKey(g)) continue;
            have.Add(g);
            var n = byGuid[g];
            Roots[0].Children.Insert(at++, CopyFrom(src, n));
            added++;
            var refs = new HashSet<string>();
            src.LocalRefs(n, refs);
            foreach (var r in refs) todo.Push(r);
        }
        return added;
    }
}

static class DbxEdit
{
    static int Main(string[] args)
    {
        if (args.Length >= 2 && args[0] == "roundtrip")
        {
            var files = Directory.Exists(args[1]) ? Directory.GetFiles(args[1], "*.dbx", SearchOption.AllDirectories) : new[] { args[1] };
            int same = 0, diff = 0, skipped = 0;
            foreach (var f in files)
            {
                Doc d;
                try { d = new Doc(f); } catch (Exception e) { Console.WriteLine("ERROR " + f + ": " + e.Message); diff++; continue; }
                if (!d.Ok) { skipped++; continue; }
                var outb = d.Save();
                if (outb.SequenceEqual(d.Original)) same++;
                else { diff++; if (diff <= 10) Console.WriteLine("DIFF " + f + " (" + d.Original.Length + " -> " + outb.Length + ")"); }
            }
            Console.WriteLine("roundtrip: " + same + " identical, " + diff + " different, " + skipped + " not binary");
            return diff == 0 ? 0 : 1;
        }
        if (args.Length >= 3 && args[0] == "edit")
        {
            var doc = new Doc(args[1]);
            if (!doc.Ok) { Console.Error.WriteLine("not a binary dbx: " + args[1]); return 2; }
            var cmds = new List<string>();
            for (int i = 3; i < args.Length; i++)
            {
                if (args[i].StartsWith("@"))
                    cmds.AddRange(File.ReadAllLines(args[i].Substring(1)).Select(l => l.Trim()).Where(l => l.Length > 0 && !l.StartsWith("#")));
                else cmds.Add(args[i]);
            }
            var sources = new Dictionary<string, Doc>();
            foreach (string c in cmds)
            {
                if (c.StartsWith("replace="))
                {
                    var p = c.Substring(8).Split('|');
                    int h = doc.Replace(p[0], p[1]);
                    Console.WriteLine("replace " + p[0] + " -> " + p[1] + ": " + h + " string(s)");
                    if (h == 0) { Console.Error.WriteLine("not found: " + p[0]); return 1; }
                }
                else if (c.StartsWith("drop="))
                {
                    int refs = 0;
                    int n = doc.Drop(doc.Roots, c.Substring(5), ref refs);
                    Console.WriteLine("drop " + c.Substring(5) + ": " + n + " instance(s), " + refs + " reference(s)");
                    if (n == 0) { Console.Error.WriteLine("no instance " + c.Substring(5)); return 1; }
                }
                else if (c.StartsWith("replaceprefix="))
                {
                    var p = c.Substring(14).Split('|');
                    int h = doc.ReplacePrefix(p[0], p[1], p.Skip(2).ToArray());
                    Console.WriteLine("replaceprefix " + p[0] + " -> " + p[1] + ": " + h + " string(s)");
                }
                else if (c.StartsWith("partguid="))
                {
                    // the partition's own guid (root element attribute), e.g. for a copy under a new name
                    var root = doc.Roots[0];
                    foreach (var at in root.Attrs) if (doc.Strings[at[0]] == "guid") at[1] = doc.Str(c.Substring(9));
                    Console.WriteLine("partguid " + c.Substring(9));
                }
                else if (c.StartsWith("copyfield="))
                {
                    // copyfield=<src.dbx>|<src instance guid>|<field>|<instance guid>
                    var p = c.Substring(10).Split('|');
                    if (!sources.ContainsKey(p[0])) sources[p[0]] = new Doc(p[0]);
                    bool ok = doc.CopyField(sources[p[0]], p[1], p[2], p[3]);
                    Console.WriteLine("copyfield " + p[2] + ": " + (ok ? "copied" : "NOT FOUND"));
                    if (!ok) return 1;
                }
                else if (c.StartsWith("set="))
                {
                    // bool / byte field (type 6): set=<field name>|true|false|<0-255>
                    var p = c.Substring(4).Split('|');
                    byte v = p[1] == "true" ? (byte)1 : p[1] == "false" ? (byte)0 : byte.Parse(p[1]);
                    int n = doc.SetByte(doc.Roots, p[0], v);
                    Console.WriteLine("set " + p[0] + " = " + p[1] + ": " + n + " field(s)");
                    if (n == 0) { Console.Error.WriteLine("no bool/byte field " + p[0]); return 1; }
                }
                else if (c.StartsWith("dropfield="))
                {
                    int n = doc.DropField(doc.Roots, c.Substring(10));
                    Console.WriteLine("dropfield " + c.Substring(10) + ": " + n + " element(s)");
                }
                else if (c.StartsWith("graft="))
                {
                    var p = c.Substring(6).Split('|');
                    if (!sources.ContainsKey(p[0])) sources[p[0]] = new Doc(p[0]);
                    int n = doc.Graft(sources[p[0]], p[1], p.Length > 2 ? p[2] : null);
                    Console.WriteLine("graft " + p[1] + " from " + Path.GetFileName(p[0]) + ": " + n + " instance(s)");
                    if (n == 0) { Console.Error.WriteLine("nothing grafted for " + p[1]); return 1; }
                }
                else { Console.Error.WriteLine("unknown command " + c); return 2; }
            }
            File.WriteAllBytes(args[2], doc.Save());
            return 0;
        }
        Console.Error.WriteLine("usage: DbxEdit roundtrip <file|folder> | edit <in.dbx> <out.dbx> <command|@file> ...");
        Console.Error.WriteLine("  replace=<old>|<new>  drop=<guid>  dropfield=<name>  graft=<source.dbx>|<guid>");
        return 2;
    }
}

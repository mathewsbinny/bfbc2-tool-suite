// DbxTool - converts BFBC2 binary .dbx ("{binary}" Frostbite 1 XML) to readable XML.
// Format and decoding rules ported from dbx.py by Frankelstner (BFBC2 Toolkit / BC2 map editor).
//
// usage: DbxTool <file.dbx|folder> [more ...]   -> writes <name>.xml next to each .dbx
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;

static class DbxTool
{
    static readonly HashSet<string> Hashes = new HashSet<string> {
        "OriginalHashedWaveName", "HashedName", "HashedWaveName", "OnRoadMaterialNameHashes", "Hash", "Id", "CompositeMeshPartNames" };

    static byte[] B;
    static int P;

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

    static uint BE(int o) { return (uint)(B[o] << 24 | B[o + 1] << 16 | B[o + 2] << 8 | B[o + 3]); }

    static string Esc(string s) { return s.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;").Replace("\"", "&quot;"); }

    // Integer if a hash, a small positive number (top byte 0) or a NaN/Inf pattern; else a float.
    static string IntFloat(int o, string name)
    {
        int i = (int)BE(o);
        if (Hashes.Contains(name) || (i >> 24) == 0 || (i >> 23) == 255 || (i >> 23) == -1) return i.ToString(CultureInfo.InvariantCulture);
        byte[] f = { B[o + 3], B[o + 2], B[o + 1], B[o] };
        return BitConverter.ToSingle(f, 0).ToString("R", CultureInfo.InvariantCulture);
    }

    static void Convert(string path)
    {
        B = File.ReadAllBytes(path);
        if (Encoding.ASCII.GetString(B, 0, 8) != "{binary}") return;
        uint reloffset = BE(16), count = BE(20);
        var offs = new uint[count];
        for (int i = 0; i < count; i++) offs[i] = BE(24 + 4 * i);
        int strBase = 24 + 4 * (int)count;
        var strings = new string[count];
        for (int i = 0; i < count; i++)
        {
            int start = strBase + (int)offs[i], end = start;
            while (end < B.Length && B[end] != 0) end++;
            strings[i] = Encoding.UTF8.GetString(B, start, end - start);
        }
        P = 24 + (int)reloffset; // payload
        var sb = new StringBuilder("<?xml version=\"1.0\"?>\r\n");
        var open = new Stack<string>();
        while (P < B.Length)
        {
            int prefixNum = R128();
            if (prefixNum == 0)
            {
                if (open.Count == 0) break;
                sb.Append(new string('\t', open.Count - 1)).Append("</").Append(open.Pop()).Append(">\r\n");
                continue;
            }
            string prefix = strings[prefixNum];
            int typ = B[P] >> 4, nattr = B[P] & 15;
            P++;
            var attrs = new List<KeyValuePair<string, string>>();
            for (int i = 0; i < nattr; i++) attrs.Add(new KeyValuePair<string, string>(strings[R128()], strings[R128()]));
            var tag = new StringBuilder(new string('\t', open.Count)).Append('<').Append(prefix);
            foreach (var a in attrs) tag.Append(' ').Append(a.Key).Append("=\"").Append(Esc(a.Value)).Append('"');
            string name = attrs.Count > 0 ? attrs[0].Value : "";
            if (typ == 0xA)
            {
                P++; // null
                sb.Append(tag).Append(">\r\n");
                open.Push(prefix);
            }
            else if (typ == 2)
            {
                string content = strings[R128()];
                if (content.Length > 0) sb.Append(tag).Append('>').Append(Esc(content)).Append("</").Append(prefix).Append(">\r\n");
                else sb.Append(tag).Append(" />\r\n");
            }
            else if (typ == 7)
            {
                int n = R128(), len = R128();
                var parts = new List<string>();
                for (int i = 0; i < n; i++)
                {
                    if (len == 4) parts.Add(IntFloat(P, name));
                    else if (len == 8) { var d = new byte[8]; for (int k = 0; k < 8; k++) d[k] = B[P + 7 - k]; parts.Add(BitConverter.ToDouble(d, 0).ToString("R", CultureInfo.InvariantCulture)); }
                    else parts.Add(((B[P] << 8) | B[P + 1]).ToString());
                    P += len;
                }
                sb.Append(tag).Append('>').Append(string.Join("/", parts)).Append("</").Append(prefix).Append(">\r\n");
            }
            else // type 6: \x01 then value byte
            {
                P++;
                byte v = B[P++];
                string content = v == 1 ? "true" : v == 0 ? "false" : v.ToString();
                sb.Append(tag).Append('>').Append(content).Append("</").Append(prefix).Append(">\r\n");
            }
        }
        File.WriteAllText(Path.ChangeExtension(path, ".xml"), sb.ToString());
    }

    static int Main(string[] args)
    {
        if (args.Length == 0) { Console.Error.WriteLine("usage: DbxTool <file.dbx|folder> [...]"); return 2; }
        int n = 0;
        foreach (var a in args)
        {
            var files = Directory.Exists(a) ? Directory.GetFiles(a, "*.dbx", SearchOption.AllDirectories) : new[] { a };
            foreach (var f in files)
            {
                try { Convert(f); n++; }
                catch (Exception ex) { Console.Error.WriteLine(f + ": " + ex.Message); }
            }
        }
        Console.WriteLine("converted " + n + " file(s)");
        return 0;
    }
}

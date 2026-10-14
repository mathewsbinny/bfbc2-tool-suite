// FbrbTool - unpack / repack Frostbite 1 ".fbrb" bundles (Battlefield: Bad Company 2).
//
// Format (as documented by Frankelstner's fbrb script, BFBC2 Toolkit by Heico):
//   "FbRB" | uint32 BE len(part1) | part1 = gzip(header) | part2 = gzip(payload) or raw payload
//   header: uint32 BE 2 | uint32 BE strlen | strings | uint32 BE count | count x 24-byte entries
//           | byte zipped | uint32 BE payload size
//   entry : name offset | flags | payload offset | size | size2 | type-name offset   (uint32 BE each)
// Offsets in entries are relative to the start of the string table.
//
// unpack writes every entry under <outdir>\ by its archive name and a "fbrb.manifest" listing the
// entries in archive order (name, type, flags, size2), so pack can rebuild the bundle faithfully.
//
// Usage: FbrbTool unpack <file.fbrb> <outdir>
//        FbrbTool pack   <dir-with-manifest> <file.fbrb>
//        FbrbTool list   <file.fbrb>
using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;

static class FbrbTool
{
    class Entry
    {
        public string Name, Type;
        public uint Flags, Offset, Size, Size2;
    }

    static uint ReadBE(byte[] b, int o) { return (uint)(b[o] << 24 | b[o + 1] << 16 | b[o + 2] << 8 | b[o + 3]); }

    static void WriteBE(Stream s, uint v)
    {
        s.WriteByte((byte)(v >> 24)); s.WriteByte((byte)(v >> 16)); s.WriteByte((byte)(v >> 8)); s.WriteByte((byte)v);
    }

    static string CString(byte[] b, int o)
    {
        int e = o;
        while (b[e] != 0) e++;
        return Encoding.ASCII.GetString(b, o, e - o);
    }

    static byte[] Gunzip(byte[] data, int offset, int count)
    {
        using (var input = new MemoryStream(data, offset, count))
        using (var gz = new GZipStream(input, CompressionMode.Decompress))
        using (var output = new MemoryStream())
        {
            gz.CopyTo(output);
            return output.ToArray();
        }
    }

    static byte[] Gzip(byte[] data)
    {
        using (var output = new MemoryStream())
        {
            using (var gz = new GZipStream(output, CompressionMode.Compress, true)) gz.Write(data, 0, data.Length);
            return output.ToArray();
        }
    }

    static void Read(string path, out List<Entry> entries, out byte[] payload, out bool zipped)
    {
        byte[] file = File.ReadAllBytes(path);
        if (file.Length < 8 || Encoding.ASCII.GetString(file, 0, 4) != "FbRB") throw new Exception(path + " is not an FbRB bundle");
        int cut = (int)ReadBE(file, 4);
        byte[] h = Gunzip(file, 8, cut);

        int strlen = (int)ReadBE(h, 4);
        const int strBase = 8;
        int count = (int)ReadBE(h, strBase + strlen);
        entries = new List<Entry>(count);
        for (int i = 0; i < count; i++)
        {
            int e = strBase + strlen + 4 + i * 24;
            entries.Add(new Entry
            {
                Name = CString(h, strBase + (int)ReadBE(h, e)),
                Flags = ReadBE(h, e + 4),
                Offset = ReadBE(h, e + 8),
                Size = ReadBE(h, e + 12),
                Size2 = ReadBE(h, e + 16),
                Type = CString(h, strBase + (int)ReadBE(h, e + 20)),
            });
        }
        int tail = strBase + strlen + 4 + count * 24;
        zipped = h[tail] != 0;
        int rest = file.Length - 8 - cut;
        if (zipped) payload = Gunzip(file, 8 + cut, rest);
        else { payload = new byte[rest]; Buffer.BlockCopy(file, 8 + cut, payload, 0, rest); }
    }

    static string Safe(string archiveName) { return archiveName.Replace('/', Path.DirectorySeparatorChar); }

    static int Unpack(string fbrb, string outDir)
    {
        List<Entry> entries; byte[] payload; bool zipped;
        Read(fbrb, out entries, out payload, out zipped);
        Directory.CreateDirectory(outDir);
        var manifest = new StringBuilder();
        manifest.AppendLine("# fbrb.manifest - entry order matters. name|type|flags(hex)|size2  zipped=" + (zipped ? 1 : 0));
        foreach (var e in entries)
        {
            string target = Path.Combine(outDir, Safe(e.Name));
            Directory.CreateDirectory(Path.GetDirectoryName(target));
            using (var f = File.Create(target)) f.Write(payload, (int)e.Offset, (int)e.Size);
            manifest.AppendLine(e.Name + "|" + e.Type + "|" + e.Flags.ToString("X8") + "|" + e.Size2);
        }
        File.WriteAllText(Path.Combine(outDir, "fbrb.manifest"), manifest.ToString());
        Console.WriteLine("unpacked " + entries.Count + " entries (" + payload.Length + " bytes) to " + outDir);
        return 0;
    }

    static int Pack(string dir, string fbrb)
    {
        string[] lines = File.ReadAllLines(Path.Combine(dir, "fbrb.manifest"));
        bool zipped = lines[0].Contains("zipped=1");
        var entries = new List<Entry>();
        var payload = new MemoryStream();
        foreach (string line in lines)
        {
            if (line.Length == 0 || line[0] == '#') continue;
            string[] p = line.Split('|');
            byte[] data = File.ReadAllBytes(Path.Combine(dir, Safe(p[0])));
            uint originalSize2 = uint.Parse(p[3]);
            var e = new Entry
            {
                Name = p[0], Type = p[1], Flags = Convert.ToUInt32(p[2], 16),
                Offset = (uint)payload.Length, Size = (uint)data.Length,
            };
            // size2 equals size for nearly every entry; a differing original value is only kept
            // when the file was not modified (same size).
            e.Size2 = (originalSize2 != e.Size && data.Length == (int)originalSize2) ? originalSize2 : e.Size;
            payload.Write(data, 0, data.Length);
            entries.Add(e);
        }

        // String table: entry names first (in order), type names deduplicated after each first use.
        var strings = new MemoryStream();
        var typeOffsets = new Dictionary<string, uint>();
        var nameOffsets = new List<uint>();
        foreach (var e in entries)
        {
            nameOffsets.Add((uint)strings.Length);
            byte[] n = Encoding.ASCII.GetBytes(e.Name + "\0");
            strings.Write(n, 0, n.Length);
            if (!typeOffsets.ContainsKey(e.Type))
            {
                typeOffsets[e.Type] = (uint)strings.Length;
                byte[] t = Encoding.ASCII.GetBytes(e.Type + "\0");
                strings.Write(t, 0, t.Length);
            }
        }

        var header = new MemoryStream();
        WriteBE(header, 2);
        WriteBE(header, (uint)strings.Length);
        strings.WriteTo(header);
        WriteBE(header, (uint)entries.Count);
        for (int i = 0; i < entries.Count; i++)
        {
            var e = entries[i];
            WriteBE(header, nameOffsets[i]); WriteBE(header, e.Flags); WriteBE(header, e.Offset);
            WriteBE(header, e.Size); WriteBE(header, e.Size2); WriteBE(header, typeOffsets[e.Type]);
        }
        header.WriteByte((byte)(zipped ? 1 : 0));
        WriteBE(header, (uint)payload.Length);

        byte[] part1 = Gzip(header.ToArray());
        byte[] part2 = zipped ? Gzip(payload.ToArray()) : payload.ToArray();
        using (var f = File.Create(fbrb))
        {
            f.Write(Encoding.ASCII.GetBytes("FbRB"), 0, 4);
            WriteBE(f, (uint)part1.Length);
            f.Write(part1, 0, part1.Length);
            f.Write(part2, 0, part2.Length);
        }
        Console.WriteLine("packed " + entries.Count + " entries (" + payload.Length + " bytes) into " + fbrb);
        return 0;
    }

    static int List(string fbrb)
    {
        List<Entry> entries; byte[] payload; bool zipped;
        Read(fbrb, out entries, out payload, out zipped);
        foreach (var e in entries) Console.WriteLine(e.Size.ToString().PadLeft(10) + "  " + e.Type.PadRight(18) + "  " + e.Name);
        Console.WriteLine(entries.Count + " entries, payload " + payload.Length + " bytes, zipped=" + zipped);
        return 0;
    }

    // SwfMovie resources are "BIGF" archives holding EA APT UI files (.apt / .const / .geo, textures).
    // Layout: "BIGF" | uint32 LE total size | uint32 BE count | uint32 BE index size |
    //         count x { uint32 BE offset | uint32 BE size | cstring name } | data
    static int BigExtract(string big, string outDir)
    {
        byte[] b = File.ReadAllBytes(big);
        string magic = Encoding.ASCII.GetString(b, 0, 4);
        if (magic != "BIGF" && magic != "BIG4") throw new Exception(big + " is not a BIG archive (" + magic + ")");
        int count = (int)ReadBE(b, 8);
        int p = 16;
        Directory.CreateDirectory(outDir);
        for (int i = 0; i < count; i++)
        {
            int offset = (int)ReadBE(b, p), size = (int)ReadBE(b, p + 4);
            string name = CString(b, p + 8);
            p += 8 + name.Length + 1;
            string target = Path.Combine(outDir, name.Replace('/', Path.DirectorySeparatorChar));
            Directory.CreateDirectory(Path.GetDirectoryName(target));
            using (var f = File.Create(target)) f.Write(b, offset, size);
            Console.WriteLine(size.ToString().PadLeft(9) + "  " + name);
        }
        return 0;
    }

    static int Main(string[] args)
    {
        try
        {
            if (args.Length == 3 && args[0] == "big") return BigExtract(args[1], args[2]);
            if (args.Length == 3 && args[0] == "unpack") return Unpack(args[1], args[2]);
            if (args.Length == 3 && args[0] == "pack") return Pack(args[1], args[2]);
            if (args.Length == 2 && args[0] == "list") return List(args[1]);
            Console.Error.WriteLine("usage: FbrbTool unpack <file.fbrb> <outdir> | pack <dir> <file.fbrb> | list <file.fbrb>");
            return 2;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("error: " + ex.Message);
            return 1;
        }
    }
}

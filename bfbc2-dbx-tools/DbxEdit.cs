// DbxEdit - reads, converts and edits Battlefield: Bad Company 2 binary .dbx files (Frostbite 1 data as a
// tokenised XML tree). Written from FORMAT.md, which describes the format and everything this tool outputs.
// Anything an edit does not change is written back byte for byte.
//
// usage: DbxEdit xml <file.dbx|folder> [...]               write <name>.xml next to every binary .dbx
//        DbxEdit roundtrip <file.dbx|folder>               read + write every file, report any that change
//        DbxEdit edit <in.dbx> <out.dbx> <command> ...      apply commands in order (FORMAT.md section 5):
//            replace=<old>|<new>     every string exactly <old> becomes <new> (re-points references)
//            drop=<guid>             removes the instance with that guid; array items referring to it are
//                                    removed, single references to it become "null"
//            replaceprefix=<old>|<new>[|<except>...]  strings starting with <old> get <new> as prefix
//            partguid=<guid>         sets the partition's own guid (a copy under a new name gets a new one)
//            copyfield=<src.dbx>|<src guid>|<field>|<guid>  copies a field (with arrays) from another file's instance
//            set=<field>|<true|false|0-255>  sets a bool / byte field
//            setfield=<guid>|<field>|<value>  sets a field of that instance (text, or one 4-byte number)
//            additem=<guid>|<array>|<ref>[|<before ref>]  adds an item referring to <ref> to array <array> of
//                                    instance <guid> (at the end, or before the item referring to <before ref>)
//            dropfield=<name>        removes every field named <name> (fields the PC build does not have)
//            graft=<src.dbx>|<guid>[|<before guid>]  copies (inserted before that instance if given) that instance of another file, with every instance of that file it
//                                    references in-partition (members), strings remapped
//        a command argument "@file" reads commands from a file (one per line, # comments)
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;

namespace Bfbc2Dbx
{
    // Varints: 7 bits per byte, low group first, high bit = more bytes follow.
    static class Varint
    {
        public static int Read(byte[] data, ref int pos)
        {
            int value = 0;
            for (int shift = 0; ; shift += 7)
            {
                byte b = data[pos++];
                value |= (b & 0x7F) << shift;
                if (b < 0x80) return value;
            }
        }

        public static void Write(List<byte> output, int value)
        {
            while (value >= 0x80 || value < 0)
            {
                output.Add((byte)(value & 0x7F | 0x80));
                value = (int)((uint)value >> 7);
            }
            output.Add((byte)value);
        }

        public static byte[] Bytes(int value)
        {
            var b = new List<byte>();
            Write(b, value);
            return b.ToArray();
        }
    }

    static class BigEndian
    {
        public static uint U32(byte[] data, int at)
        {
            return (uint)data[at] << 24 | (uint)data[at + 1] << 16 | (uint)data[at + 2] << 8 | data[at + 3];
        }

        public static void Put(List<byte> output, uint value)
        {
            for (int shift = 24; shift >= 0; shift -= 8) output.Add((byte)(value >> shift));
        }
    }

    sealed class Attribute
    {
        public int Name, Value;   // string-table indexes
        public Attribute(int name, int value) { Name = name; Value = value; }
    }

    // One element of the tree (FORMAT.md section 2). Containers have Children; other kinds keep their value
    // bytes exactly as stored (Payload).
    sealed class Element
    {
        public const int Container = 0xA, Text = 2, Numbers = 7;
        public int Tag;
        public int Kind;
        public readonly List<Attribute> Attributes = new List<Attribute>();
        public byte ContainerByte;
        public List<Element> Children;
        public byte[] Payload;

        public bool IsContainer { get { return Children != null; } }
    }

    // A parsed binary .dbx file.
    sealed class DbxFile
    {
        const string Magic = "{binary}";

        public readonly byte[] Original;
        public readonly bool IsBinary;
        public List<string> Table;
        public bool TableChanged;
        public List<Element> TopLevel;
        uint keptHeaderValue;
        List<int> originalOffsets;
        byte[] originalStringArea;   // offsets table excluded: the string bytes as stored
        byte[] trailer;              // the closing 0 of the tree and anything after it

        public DbxFile(string path) : this(File.ReadAllBytes(path)) { }

        public DbxFile(byte[] data)
        {
            Original = data;
            IsBinary = data.Length > 24 && Encoding.ASCII.GetString(data, 0, 8) == Magic;
            if (!IsBinary) return;
            uint areaSize = BigEndian.U32(data, 16), count = BigEndian.U32(data, 20);
            keptHeaderValue = BigEndian.U32(data, 12);
            int stringsAt = 24 + 4 * (int)count, payloadAt = 24 + (int)areaSize;
            originalOffsets = new List<int>();
            for (int i = 0; i < count; i++) originalOffsets.Add((int)BigEndian.U32(data, 24 + 4 * i));
            Table = new List<string>(originalOffsets.Count);
            foreach (int offset in originalOffsets)
            {
                int from = stringsAt + offset, to = from;
                while (data[to] != 0) to++;
                Table.Add(Encoding.UTF8.GetString(data, from, to - from));
            }
            originalStringArea = new byte[payloadAt - stringsAt];
            Array.Copy(data, stringsAt, originalStringArea, 0, originalStringArea.Length);

            TopLevel = new List<Element>();
            int pos = payloadAt;
            while (pos < data.Length)
            {
                int tag = Varint.Read(data, ref pos);
                if (tag == 0) { pos--; break; }
                TopLevel.Add(ReadElement(data, ref pos, tag));
            }
            trailer = new byte[data.Length - pos];
            Array.Copy(data, pos, trailer, 0, trailer.Length);
        }

        static Element ReadElement(byte[] data, ref int pos, int tag)
        {
            var e = new Element { Tag = tag };
            byte head = data[pos++];
            e.Kind = head >> 4;
            for (int count = head & 0xF; count > 0; count--)
            {
                int name = Varint.Read(data, ref pos);
                e.Attributes.Add(new Attribute(name, Varint.Read(data, ref pos)));
            }
            if (e.Kind == Element.Container)
            {
                e.ContainerByte = data[pos++];
                e.Children = new List<Element>();
                for (int child = Varint.Read(data, ref pos); child != 0; child = Varint.Read(data, ref pos))
                    e.Children.Add(ReadElement(data, ref pos, child));
                return e;
            }
            int valueAt = pos;
            if (e.Kind == Element.Text) Varint.Read(data, ref pos);
            else if (e.Kind == Element.Numbers)
            {
                int n = Varint.Read(data, ref pos), size = Varint.Read(data, ref pos);
                pos += n * size;
            }
            else pos += 2;
            e.Payload = new byte[pos - valueAt];
            Array.Copy(data, valueAt, e.Payload, 0, e.Payload.Length);
            return e;
        }

        public byte[] Serialize()
        {
            var stringArea = originalStringArea;
            var offsets = originalOffsets;
            if (TableChanged)
            {
                var bytes = new List<byte>();
                offsets = new List<int>(Table.Count);
                foreach (string s in Table)
                {
                    offsets.Add(bytes.Count);
                    bytes.AddRange(Encoding.UTF8.GetBytes(s));
                    bytes.Add(0);
                }
                stringArea = bytes.ToArray();
            }
            var tree = new List<byte>();
            foreach (var e in TopLevel) WriteElement(tree, e);
            tree.AddRange(trailer);

            uint areaSize = (uint)(4 * Table.Count + stringArea.Length);
            var output = new List<byte>(Encoding.ASCII.GetBytes(Magic));
            BigEndian.Put(output, 24 + areaSize);
            BigEndian.Put(output, keptHeaderValue);
            BigEndian.Put(output, areaSize);
            BigEndian.Put(output, (uint)Table.Count);
            foreach (int offset in offsets) BigEndian.Put(output, (uint)offset);
            output.AddRange(stringArea);
            output.AddRange(tree);
            return output.ToArray();
        }

        static void WriteElement(List<byte> output, Element e)
        {
            Varint.Write(output, e.Tag);
            output.Add((byte)(e.Kind << 4 | e.Attributes.Count));
            foreach (var a in e.Attributes)
            {
                Varint.Write(output, a.Name);
                Varint.Write(output, a.Value);
            }
            if (e.Kind != Element.Container)
            {
                output.AddRange(e.Payload);
                return;
            }
            output.Add(e.ContainerByte);
            foreach (var c in e.Children) WriteElement(output, c);
            output.Add(0);
        }

        // The index of a string: the first identical table entry, or a new one at the end.
        public int Intern(string s)
        {
            int at = Table.IndexOf(s);
            if (at >= 0) return at;
            Table.Add(s);
            TableChanged = true;
            return Table.Count - 1;
        }

        public string TagOf(Element e) { return Table[e.Tag]; }

        // Value of the first attribute with that name, or null.
        public string Get(Element e, string attribute)
        {
            var a = e.Attributes.FirstOrDefault(x => Table[x.Name] == attribute);
            return a == null ? null : Table[a.Value];
        }

        public IEnumerable<Element> Instances()
        {
            return TopLevel[0].Children.Where(e => TagOf(e) == "instance");
        }

        public Element Instance(string guid)
        {
            return Instances().FirstOrDefault(e => Guid.Same(Get(e, "guid"), guid));
        }
    }

    static class Guid
    {
        public static bool Same(string a, string b) { return string.Equals(a, b, StringComparison.OrdinalIgnoreCase); }
    }

    // FORMAT.md section 3: binary .dbx -> readable XML.
    static class XmlExport
    {
        static readonly HashSet<string> WholeNumberFields = new HashSet<string> {
            "OriginalHashedWaveName", "HashedName", "HashedWaveName", "OnRoadMaterialNameHashes", "Hash", "Id",
            "CompositeMeshPartNames" };

        public static string Convert(byte[] data)
        {
            uint areaSize = BigEndian.U32(data, 16), count = BigEndian.U32(data, 20);
            int stringsAt = 24 + 4 * (int)count;
            var offsets = new uint[count];
            for (int i = 0; i < count; i++) offsets[i] = BigEndian.U32(data, 24 + 4 * i);
            var table = new string[count];
            for (int i = 0; i < count; i++)
            {
                int from = stringsAt + (int)offsets[i], to = from;
                while (to < data.Length && data[to] != 0) to++;
                table[i] = Encoding.UTF8.GetString(data, from, to - from);
            }

            var xml = new StringBuilder("<?xml version=\"1.0\"?>\r\n");
            var open = new Stack<string>();
            int pos = 24 + (int)areaSize;
            while (pos < data.Length)
            {
                int tag = Varint.Read(data, ref pos);
                if (tag == 0)
                {
                    if (open.Count == 0) break;
                    string closing = open.Pop();
                    xml.Append('\t', open.Count).Append("</").Append(closing).Append(">\r\n");
                    continue;
                }
                string name = table[tag];
                byte head = data[pos++];
                int kind = head >> 4;
                xml.Append('\t', open.Count).Append('<').Append(name);
                string firstValue = "";
                for (int i = 0, n = head & 0xF; i < n; i++)
                {
                    string key = table[Varint.Read(data, ref pos)], value = table[Varint.Read(data, ref pos)];
                    if (i == 0) firstValue = value;
                    xml.Append(' ').Append(key).Append("=\"").Append(Escape(value)).Append('"');
                }
                if (kind == Element.Container)
                {
                    pos++;
                    xml.Append(">\r\n");
                    open.Push(name);
                    continue;
                }
                string content;
                if (kind == Element.Text)
                {
                    content = Escape(table[Varint.Read(data, ref pos)]);
                    if (content.Length == 0)
                    {
                        xml.Append(" />\r\n");
                        continue;
                    }
                }
                else if (kind == Element.Numbers)
                {
                    int n = Varint.Read(data, ref pos), size = Varint.Read(data, ref pos);
                    var values = new string[n];
                    for (int i = 0; i < n; i++, pos += size) values[i] = Number(data, pos, size, firstValue);
                    content = string.Join("/", values);
                }
                else
                {
                    byte v = data[pos + 1];
                    pos += 2;
                    content = v == 0 ? "false" : v == 1 ? "true" : v.ToString();
                }
                xml.Append('>').Append(content).Append("</").Append(name).Append(">\r\n");
            }
            return xml.ToString();
        }

        static string Number(byte[] data, int at, int size, string field)
        {
            if (size == 4)
            {
                int whole = (int)BigEndian.U32(data, at);
                int exponent = (whole >> 23) & 0xFF;
                if (WholeNumberFields.Contains(field) || (whole >> 24) == 0 || exponent == 0xFF)
                    return whole.ToString(CultureInfo.InvariantCulture);
                return BitConverter.ToSingle(BitConverter.GetBytes(whole), 0).ToString("R", CultureInfo.InvariantCulture);
            }
            if (size == 8)
            {
                long bits = (long)BigEndian.U32(data, at) << 32 | BigEndian.U32(data, at + 4);
                return BitConverter.Int64BitsToDouble(bits).ToString("R", CultureInfo.InvariantCulture);
            }
            return (data[at] << 8 | data[at + 1]).ToString();
        }

        static string Escape(string s)
        {
            if (s.IndexOfAny(new[] { '&', '<', '>', '"' }) < 0) return s;
            var b = new StringBuilder(s.Length + 16);
            foreach (char c in s)
            {
                switch (c)
                {
                    case '&': b.Append("&amp;"); break;
                    case '<': b.Append("&lt;"); break;
                    case '>': b.Append("&gt;"); break;
                    case '"': b.Append("&quot;"); break;
                    default: b.Append(c); break;
                }
            }
            return b.ToString();
        }
    }

    // FORMAT.md section 5: the edit commands. Each returns a count or a success flag; Program prints.
    sealed class Editor
    {
        readonly DbxFile file;
        public Editor(DbxFile file) { this.file = file; }

        public int Replace(string from, string to)
        {
            int hits = 0;
            for (int i = 0; i < file.Table.Count; i++)
            {
                if (file.Table[i] != from) continue;
                file.Table[i] = to;
                file.TableChanged = true;
                hits++;
            }
            return hits;
        }

        public int ReplacePrefix(string from, string to, IList<string> except)
        {
            int hits = 0;
            for (int i = 0; i < file.Table.Count; i++)
            {
                string s = file.Table[i];
                if (!s.StartsWith(from, StringComparison.OrdinalIgnoreCase)) continue;
                if (except.Any(x => x.Length > 0 && s.StartsWith(x, StringComparison.OrdinalIgnoreCase))) continue;
                string head = s.Substring(0, from.Length);
                file.Table[i] = (head == head.ToLowerInvariant() ? to.ToLowerInvariant() : to) + s.Substring(from.Length);
                file.TableChanged = true;
                hits++;
            }
            return hits;
        }

        // Returns instances removed; references counts the references met.
        public int Drop(string guid, out int references)
        {
            references = 0;
            return Drop(file.TopLevel, guid, ref references);
        }

        int Drop(List<Element> list, string guid, ref int references)
        {
            int removed = 0;
            for (int i = list.Count - 1; i >= 0; i--)
            {
                var e = list[i];
                string tag = file.TagOf(e);
                if (tag == "instance" && Guid.Same(file.Get(e, "guid"), guid))
                {
                    list.RemoveAt(i);
                    removed++;
                    continue;
                }
                string target = file.Get(e, "ref");
                if (target != null && (Guid.Same(target, guid) || target.EndsWith("/" + guid, StringComparison.OrdinalIgnoreCase)))
                {
                    references++;
                    if (tag == "item")
                    {
                        list.RemoveAt(i);
                        continue;
                    }
                    foreach (var a in e.Attributes.Where(a => file.Table[a.Name] == "ref")) a.Value = file.Intern("null");
                }
                if (e.IsContainer) removed += Drop(e.Children, guid, ref references);
            }
            return removed;
        }

        public int DropField(string name) { return DropField(file.TopLevel, name); }

        int DropField(List<Element> list, string name)
        {
            int removed = 0;
            for (int i = list.Count - 1; i >= 0; i--)
            {
                var e = list[i];
                if (file.TagOf(e) != "instance" && file.Get(e, "name") == name)
                {
                    list.RemoveAt(i);
                    removed++;
                }
                else if (e.IsContainer) removed += DropField(e.Children, name);
            }
            return removed;
        }

        public int SetByte(string name, byte value) { return SetByte(file.TopLevel, name, value); }

        int SetByte(List<Element> list, string name, byte value)
        {
            int set = 0;
            foreach (var e in list)
            {
                if (e.IsContainer) set += SetByte(e.Children, name, value);
                else if (e.Kind != Element.Text && e.Kind != Element.Numbers && e.Payload != null && e.Payload.Length == 2
                         && file.Get(e, "name") == name)
                {
                    e.Payload = new[] { e.Payload[0], value };
                    set++;
                }
            }
            return set;
        }

        public bool SetField(string guid, string field, string value)
        {
            var instance = file.Instance(guid);
            if (instance == null || !instance.IsContainer) return false;
            var e = instance.Children.FirstOrDefault(c => file.Get(c, "name") == field && (c.Kind == Element.Text || c.Kind == Element.Numbers));
            if (e == null) return false;
            if (e.Kind == Element.Text)
            {
                e.Payload = Varint.Bytes(file.Intern(value));
                return true;
            }
            // one 4-byte number: count 1, size 4, big-endian value (float if the text has a '.')
            if (e.Payload.Length != 6 || e.Payload[0] != 1 || e.Payload[1] != 4) return false;
            uint bits = value.Contains(".")
                ? BitConverter.ToUInt32(BitConverter.GetBytes(float.Parse(value, CultureInfo.InvariantCulture)), 0)
                : (uint)int.Parse(value);
            e.Payload = new byte[] { 1, 4, (byte)(bits >> 24), (byte)(bits >> 16), (byte)(bits >> 8), (byte)bits };
            return true;
        }

        public void SetPartitionGuid(string guid)
        {
            foreach (var a in file.TopLevel[0].Attributes.Where(a => file.Table[a.Name] == "guid")) a.Value = file.Intern(guid);
        }

        public bool AddItem(string guid, string arrayName, string reference, string before)
        {
            var instance = file.Instance(guid);
            if (instance == null) return false;
            var array = instance.Children.FirstOrDefault(c => file.TagOf(c) == "array" && file.Get(c, "name") == arrayName);
            if (array == null || !array.IsContainer) return false;
            var shape = array.Children.FirstOrDefault(c => file.TagOf(c) == "item");
            if (shape == null) return false;
            var item = new Element { Tag = shape.Tag, Kind = shape.Kind, ContainerByte = shape.ContainerByte, Payload = shape.Payload };
            item.Attributes.Add(new Attribute(file.Intern("ref"), file.Intern(reference)));
            if (shape.IsContainer) item.Children = new List<Element>();
            int at = before == null ? -1 : array.Children.FindIndex(c => Guid.Same(file.Get(c, "ref"), before));
            array.Children.Insert(at >= 0 ? at : array.Children.Count, item);
            return true;
        }

        public bool CopyField(DbxFile source, string sourceGuid, string field, string guid)
        {
            var from = source.Instance(sourceGuid);
            var to = file.Instance(guid);
            if (from == null || to == null) return false;
            var original = FindField(source, from, field);
            if (original == null) return false;
            var copy = Import(source, original);
            if (!ReplaceField(to, field, copy)) to.Children.Add(copy);
            return true;
        }

        // First element named <field> below e that is not an instance (each element checked before its children).
        static Element FindField(DbxFile f, Element e, string field)
        {
            if (!e.IsContainer) return null;
            foreach (var c in e.Children)
            {
                if (f.TagOf(c) != "instance" && f.Get(c, "name") == field) return c;
                var deeper = FindField(f, c, field);
                if (deeper != null) return deeper;
            }
            return null;
        }

        bool ReplaceField(Element e, string field, Element replacement)
        {
            if (!e.IsContainer) return false;
            for (int i = 0; i < e.Children.Count; i++)
            {
                if (file.Get(e.Children[i], "name") == field)
                {
                    e.Children[i] = replacement;
                    return true;
                }
                if (ReplaceField(e.Children[i], field, replacement)) return true;
            }
            return false;
        }

        // Copies an element of another file into this one. Strings are interned in this order: tag, each
        // attribute's name then value, a text value, then the children (FORMAT.md section 5).
        Element Import(DbxFile source, Element e)
        {
            var copy = new Element { Tag = file.Intern(source.Table[e.Tag]), Kind = e.Kind, ContainerByte = e.ContainerByte };
            foreach (var a in e.Attributes)
            {
                int name = file.Intern(source.Table[a.Name]);
                copy.Attributes.Add(new Attribute(name, file.Intern(source.Table[a.Value])));
            }
            if (e.Kind == Element.Text)
            {
                int pos = 0;
                copy.Payload = Varint.Bytes(file.Intern(source.Table[Varint.Read(e.Payload, ref pos)]));
            }
            else copy.Payload = e.Payload;
            if (e.IsContainer) copy.Children = e.Children.Select(c => Import(source, c)).ToList();
            return copy;
        }

        // Copies instance <guid> of source and, depth first, every source instance it references locally that
        // this file does not have yet; inserted before instance <before>, or at the end of the partition.
        public int Graft(DbxFile source, string guid, string before)
        {
            var partition = file.TopLevel[0].Children;
            int at = partition.Count;
            if (before != null)
            {
                int i = partition.FindIndex(e => file.TagOf(e) == "instance" && Guid.Same(file.Get(e, "guid"), before));
                if (i >= 0) at = i;
            }
            var available = new Dictionary<string, Element>();
            foreach (var e in source.Instances()) available[source.Get(e, "guid").ToUpperInvariant()] = e;
            var present = new HashSet<string>(file.Instances().Select(e => file.Get(e, "guid").ToUpperInvariant()));
            var pending = new Stack<string>();
            pending.Push(guid.ToUpperInvariant());
            int copied = 0;
            while (pending.Count > 0)
            {
                string next = pending.Pop();
                Element e;
                if (present.Contains(next) || !available.TryGetValue(next, out e)) continue;
                present.Add(next);
                partition.Insert(at++, Import(source, e));
                copied++;
                foreach (string r in LocalReferences(source, e)) pending.Push(r);
            }
            return copied;
        }

        // Local references (36 characters, no '/') below e, upper-cased, each once, in document order.
        static List<string> LocalReferences(DbxFile f, Element e)
        {
            var found = new List<string>();
            var seen = new HashSet<string>();
            Collect(f, e, found, seen);
            return found;
        }

        static void Collect(DbxFile f, Element e, List<string> found, HashSet<string> seen)
        {
            string r = f.Get(e, "ref");
            if (r != null && r.Length == 36 && r.IndexOf('/') < 0 && seen.Add(r.ToUpperInvariant())) found.Add(r.ToUpperInvariant());
            if (e.IsContainer) foreach (var c in e.Children) Collect(f, c, found, seen);
        }
    }

    static class Program
    {
        static int Main(string[] args)
        {
            if (args.Length >= 2 && args[0] == "xml") return Xml(args.Skip(1));
            if (args.Length >= 2 && args[0] == "roundtrip") return Roundtrip(args[1]);
            if (args.Length >= 3 && args[0] == "edit") return Edit(args);
            Console.Error.WriteLine("usage: DbxEdit xml <file|folder> ... | roundtrip <file|folder> | edit <in.dbx> <out.dbx> <command|@file> ...");
            Console.Error.WriteLine("  replace=<old>|<new>  drop=<guid>  dropfield=<name>  graft=<source.dbx>|<guid>");
            return 2;
        }

        static string[] DbxFiles(string path)
        {
            return Directory.Exists(path) ? Directory.GetFiles(path, "*.dbx", SearchOption.AllDirectories) : new[] { path };
        }

        static int Xml(IEnumerable<string> paths)
        {
            int converted = 0;
            foreach (string f in paths.SelectMany(DbxFiles))
            {
                try
                {
                    byte[] data = File.ReadAllBytes(f);
                    if (Encoding.ASCII.GetString(data, 0, 8) == "{binary}")
                        File.WriteAllText(Path.ChangeExtension(f, ".xml"), XmlExport.Convert(data));
                    converted++;
                }
                catch (Exception e)
                {
                    Console.Error.WriteLine(f + ": " + e.Message);
                }
            }
            Console.WriteLine("converted " + converted + " file(s)");
            return 0;
        }

        static int Roundtrip(string path)
        {
            int same = 0, different = 0, skipped = 0;
            foreach (string f in DbxFiles(path))
            {
                DbxFile d;
                try { d = new DbxFile(f); }
                catch (Exception e)
                {
                    Console.WriteLine("ERROR " + f + ": " + e.Message);
                    different++;
                    continue;
                }
                if (!d.IsBinary) { skipped++; continue; }
                byte[] written = d.Serialize();
                if (written.SequenceEqual(d.Original)) same++;
                else if (++different <= 10) Console.WriteLine("DIFF " + f + " (" + d.Original.Length + " -> " + written.Length + ")");
            }
            Console.WriteLine("roundtrip: " + same + " identical, " + different + " different, " + skipped + " not binary");
            return different == 0 ? 0 : 1;
        }

        static int Edit(string[] args)
        {
            var file = new DbxFile(args[1]);
            if (!file.IsBinary) { Console.Error.WriteLine("not a binary dbx: " + args[1]); return 2; }
            var commands = new List<string>();
            foreach (string a in args.Skip(3))
            {
                if (a.StartsWith("@"))
                    commands.AddRange(File.ReadAllLines(a.Substring(1)).Select(l => l.Trim()).Where(l => l.Length > 0 && !l.StartsWith("#")));
                else commands.Add(a);
            }
            var editor = new Editor(file);
            var sources = new Dictionary<string, DbxFile>();
            Func<string, DbxFile> source = p => { DbxFile s; if (!sources.TryGetValue(p, out s)) sources[p] = s = new DbxFile(p); return s; };
            int fieldsSet = 0;
            foreach (string c in commands)
            {
                int eq = c.IndexOf('=');
                string verb = eq < 0 ? c : c.Substring(0, eq + 1);
                string[] p = eq < 0 ? new string[0] : c.Substring(eq + 1).Split('|');
                switch (verb)
                {
                    case "replace=":
                    {
                        int hits = editor.Replace(p[0], p[1]);
                        Console.WriteLine("replace " + p[0] + " -> " + p[1] + ": " + hits + " string(s)");
                        if (hits == 0) { Console.Error.WriteLine("not found: " + p[0]); return 1; }
                        break;
                    }
                    case "drop=":
                    {
                        int references;
                        string guid = c.Substring(eq + 1);
                        int removed = editor.Drop(guid, out references);
                        Console.WriteLine("drop " + guid + ": " + removed + " instance(s), " + references + " reference(s)");
                        if (removed == 0) { Console.Error.WriteLine("no instance " + guid); return 1; }
                        break;
                    }
                    case "replaceprefix=":
                        Console.WriteLine("replaceprefix " + p[0] + " -> " + p[1] + ": " + editor.ReplacePrefix(p[0], p[1], p.Skip(2).ToList()) + " string(s)");
                        break;
                    case "partguid=":
                        editor.SetPartitionGuid(c.Substring(eq + 1));
                        Console.WriteLine("partguid " + c.Substring(eq + 1));
                        break;
                    case "copyfield=":
                    {
                        bool ok = editor.CopyField(source(p[0]), p[1], p[2], p[3]);
                        Console.WriteLine("copyfield " + p[2] + ": " + (ok ? "copied" : "NOT FOUND"));
                        if (!ok) return 1;
                        break;
                    }
                    case "set=":
                    {
                        byte v = p[1] == "true" ? (byte)1 : p[1] == "false" ? (byte)0 : byte.Parse(p[1]);
                        int n = editor.SetByte(p[0], v);
                        Console.WriteLine("set " + p[0] + " = " + p[1] + ": " + n + " field(s)");
                        if (n == 0) { Console.Error.WriteLine("no bool/byte field " + p[0]); return 1; }
                        break;
                    }
                    case "setfield=":
                        if (!editor.SetField(p[0], p[1], p[2])) { Console.Error.WriteLine("setfield: no settable field " + p[1] + " in " + p[0]); return 1; }
                        fieldsSet++;
                        break;
                    case "additem=":
                    {
                        bool ok = editor.AddItem(p[0], p[1], p[2], p.Length > 3 ? p[3] : null);
                        Console.WriteLine("additem " + p[2] + " -> " + p[1] + ": " + (ok ? "added" : "NOT FOUND"));
                        if (!ok) return 1;
                        break;
                    }
                    case "dropfield=":
                        Console.WriteLine("dropfield " + c.Substring(eq + 1) + ": " + editor.DropField(c.Substring(eq + 1)) + " element(s)");
                        break;
                    case "graft=":
                    {
                        int n = editor.Graft(source(p[0]), p[1], p.Length > 2 ? p[2] : null);
                        Console.WriteLine("graft " + p[1] + " from " + Path.GetFileName(p[0]) + ": " + n + " instance(s)");
                        if (n == 0 && file.Instance(p[1]) == null) { Console.Error.WriteLine("nothing grafted for " + p[1]); return 1; }
                        break;
                    }
                    default:
                        Console.Error.WriteLine("unknown command " + c);
                        return 2;
                }
            }
            if (fieldsSet > 0) Console.WriteLine("setfield: " + fieldsSet + " field(s)");
            File.WriteAllBytes(args[2], file.Serialize());
            return 0;
        }
    }
}

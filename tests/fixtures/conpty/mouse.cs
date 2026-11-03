using System;
using System.Runtime.InteropServices;
using System.Text;

public static class CcMouse
{
    [StructLayout(LayoutKind.Explicit, Size = 20)]
    struct Record
    {
        [FieldOffset(0)] public ushort Type;
        [FieldOffset(4)] public short X;
        [FieldOffset(6)] public short Y;
        [FieldOffset(8)] public uint Buttons;
    }

    [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int which);
    [DllImport("kernel32.dll")] static extern bool GetConsoleMode(IntPtr handle, out uint mode);
    [DllImport("kernel32.dll")] static extern bool SetConsoleMode(IntPtr handle, uint mode);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool ReadConsoleW(IntPtr handle, StringBuilder text, uint size, out uint read, IntPtr control);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool ReadConsoleInputW(IntPtr handle, [Out] Record[] records, uint size, out uint read);

    static IntPtr input = GetStdHandle(-10);

    static void Say(string text)
    {
        IntPtr output = GetStdHandle(-11);
        uint mode;
        GetConsoleMode(output, out mode);
        SetConsoleMode(output, mode | 5);
        Console.Write(text + "\r\n");
    }

    public static void Vt()
    {
        uint mode;
        GetConsoleMode(input, out mode);
        SetConsoleMode(input, 0x280);
        Say("\u001b[?1000h\u001b[?1006hcc-ready");
        StringBuilder read = new StringBuilder();
        while (!read.ToString().EndsWith("m"))
        {
            StringBuilder chunk = new StringBuilder(64);
            uint count;
            ReadConsoleW(input, chunk, 64, out count, IntPtr.Zero);
            read.Append(chunk.ToString(0, (int)count));
        }
        Say("\u001b[?1006l\u001b[?1000lcc-read " + read.ToString().Replace("\u001b", "ESC"));
        SetConsoleMode(input, mode);
    }

    public static void Records()
    {
        uint mode;
        GetConsoleMode(input, out mode);
        SetConsoleMode(input, 0x98);
        Say("cc-ready");
        Record[] records = new Record[16];
        while (true)
        {
            uint count;
            ReadConsoleInputW(input, records, 16, out count);
            for (int i = 0; i < count; i++)
            {
                if (records[i].Type == 2 && records[i].Buttons == 1)
                {
                    SetConsoleMode(input, mode);
                    Say("cc-click " + records[i].X + "," + records[i].Y);
                    return;
                }
            }
        }
    }
}

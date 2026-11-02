using System.Runtime.InteropServices;
using System.Text;

namespace MintyLauncher.Services;

public sealed class WindowsCredentialRefreshTokenStore : IRefreshTokenStore
{
    private const string CredentialTarget = "MintyLauncher/Minecraft/MicrosoftRefreshToken";
    private const uint GenericCredentialType = 1;
    private const uint LocalMachinePersistence = 2;

    public bool IsSupported => OperatingSystem.IsWindows();

    public Task<string?> ReadAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        EnsureSupported();
        if (!NativeMethods.CredRead(CredentialTarget, GenericCredentialType, out var credentialPointer, 0))
        {
            var error = Marshal.GetLastWin32Error();
            if (error == NativeMethods.ErrorNotFound)
            {
                return Task.FromResult<string?>(null);
            }

            throw new InvalidOperationException("The saved Minecraft sign-in could not be read.");
        }

        try
        {
            var credential = Marshal.PtrToStructure<NativeCredential>(credentialPointer);
            if (credential.CredentialBlob == IntPtr.Zero || credential.CredentialBlobSize == 0)
            {
                return Task.FromResult<string?>(null);
            }

            var bytes = new byte[credential.CredentialBlobSize];
            try
            {
                Marshal.Copy(credential.CredentialBlob, bytes, 0, bytes.Length);
                return Task.FromResult<string?>(Encoding.UTF8.GetString(bytes));
            }
            finally
            {
                Array.Clear(bytes);
            }
        }
        finally
        {
            NativeMethods.CredFree(credentialPointer);
        }
    }

    public Task WriteAsync(string refreshToken, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(refreshToken);
        cancellationToken.ThrowIfCancellationRequested();
        EnsureSupported();

        var blob = Encoding.UTF8.GetBytes(refreshToken);
        var blobPointer = Marshal.AllocHGlobal(blob.Length);
        var credentialPointer = IntPtr.Zero;
        try
        {
            Marshal.Copy(blob, 0, blobPointer, blob.Length);
            var credential = new NativeCredential
            {
                Type = GenericCredentialType,
                TargetName = CredentialTarget,
                CredentialBlobSize = checked((uint)blob.Length),
                CredentialBlob = blobPointer,
                Persist = LocalMachinePersistence,
                UserName = "Minty Launcher"
            };
            credentialPointer = Marshal.AllocHGlobal(Marshal.SizeOf<NativeCredential>());
            Marshal.StructureToPtr(credential, credentialPointer, false);

            if (!NativeMethods.CredWrite(credentialPointer, 0))
            {
                throw new InvalidOperationException("The Minecraft sign-in could not be saved.");
            }
        }
        finally
        {
            if (credentialPointer != IntPtr.Zero)
            {
                Marshal.DestroyStructure<NativeCredential>(credentialPointer);
                Marshal.FreeHGlobal(credentialPointer);
            }
            Marshal.Copy(new byte[blob.Length], 0, blobPointer, blob.Length);
            Marshal.FreeHGlobal(blobPointer);
            Array.Clear(blob);
        }

        return Task.CompletedTask;
    }

    public Task DeleteAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        EnsureSupported();
        if (!NativeMethods.CredDelete(CredentialTarget, GenericCredentialType, 0))
        {
            var error = Marshal.GetLastWin32Error();
            if (error != NativeMethods.ErrorNotFound)
            {
                throw new InvalidOperationException("The saved Minecraft sign-in could not be removed.");
            }
        }

        return Task.CompletedTask;
    }

    private static void EnsureSupported()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException(
                "Minecraft sign-in storage is supported only on Windows.");
        }
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NativeCredential
    {
        public uint Flags;
        public uint Type;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string TargetName;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? Comment;

        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist;
        public uint AttributeCount;
        public IntPtr Attributes;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? TargetAlias;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? UserName;
    }

    private static class NativeMethods
    {
        internal const int ErrorNotFound = 1168;

        [DllImport("Advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool CredRead(
            string targetName,
            uint type,
            out IntPtr credential,
            uint flags);

        [DllImport("Advapi32.dll", EntryPoint = "CredWriteW", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool CredWrite(IntPtr credential, uint flags);

        [DllImport("Advapi32.dll", EntryPoint = "CredDeleteW", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool CredDelete(string targetName, uint type, uint flags);

        [DllImport("Advapi32.dll", EntryPoint = "CredFree")]
        internal static extern void CredFree(IntPtr buffer);
    }
}

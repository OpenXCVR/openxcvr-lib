// Shim: C# 9 records / init-only setters target netstandard2.1, where
// System.Runtime.CompilerServices.IsExternalInit does not exist (it ships
// with .NET 5+). Declaring it internally is the standard polyfill.
namespace System.Runtime.CompilerServices
{
    internal static class IsExternalInit
    {
    }
}

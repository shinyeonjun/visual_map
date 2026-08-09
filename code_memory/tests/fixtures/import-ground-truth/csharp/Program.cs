using ImportGroundTruth.Local;
using static ImportGroundTruth.Local.Helper;
using static ImportGroundTruth.Ambiguous.Shared;
using System.Text;
using Missing.Product;
// using Commented.Fake;

namespace ImportGroundTruth.App;

public static class Program
{
    public static void Main() => Console.WriteLine($"{GetValue()}:{One + Two}:{Encoding.UTF8}");
}

// Reports over the warehouse: inventory valuation, order backlog and error
// summaries, rendered as aligned text or CSV. Reads through the services
// and repositories; never writes.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using Warehouse.Data.Specialized;
using Warehouse.Errors;
using Warehouse.Models;
using Warehouse.Services;
using Table = System.Collections.Generic.List<string[]>;

namespace Warehouse.Reporting
{
    /// <summary>Marks a row property as a report column.</summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class ColumnAttribute(string header, int width = 0) : Attribute
    {
        public string Header { get; } = header;

        public int Width { get; } = width;
    }

    public interface IRow
    {
        string[] Cells();
    }

    public interface IRenderer
    {
        string Name { get; }

        void Render(TextWriter output, string title, string[] headers, Table rows);
    }

    /// <summary>One product's stock and value.</summary>
    public sealed record InventoryRow(
        [property: Column("SKU", 8)] Sku Sku,
        [property: Column("Product", 24)] string Name,
        [property: Column("Qty", 6)] int Quantity,
        [property: Column("Value", 12)] Price Value) : IRow
    {
        public string[] Cells() => new[] { Sku.Value, Name, Quantity.ToString(CultureInfo.InvariantCulture), Value.ToString() };
    }

    public sealed record BacklogRow(Guid Id, string Customer, OrderStatus Status, int Lines, Price Total) : IRow
    {
        public string[] Cells() => new[] { Id.ToString("N")[..8], Customer, Status.ToString(), Lines.ToString(), Total.ToString() };
    }

    /// <summary>A report: rows of one type, a title and a renderer.</summary>
    public abstract class Report<TRow>
        where TRow : IRow
    {
        protected Report(string title) => Title = title;

        public string Title { get; }

        public abstract string[] Headers { get; }

        protected abstract IEnumerable<TRow> Rows();

        public string RenderWith(IRenderer renderer)
        {
            using var writer = new StringWriter(CultureInfo.InvariantCulture);
            var rows = new Table();
            rows.AddRange(Rows().Select(r => r.Cells()));
            renderer.Render(writer, Title, Headers, rows);
            return writer.ToString();
        }

        public override string ToString() => RenderWith(Renderers.Text);
    }

    public sealed class InventoryReport : Report<InventoryRow>
    {
        private readonly IReadOnlyList<Product> _products;
        private readonly IReadOnlyDictionary<Sku, int> _counts;

        public InventoryReport(IReadOnlyList<Product> products, IReadOnlyDictionary<Sku, int> counts)
            : base("Inventory valuation")
        {
            _products = products;
            _counts = counts;
        }

        public override string[] Headers => new[] { "SKU", "Product", "Qty", "Value" };

        protected override IEnumerable<InventoryRow> Rows()
        {
            foreach (var product in _products.OrderBy(p => p.Id.Value))
            {
                var quantity = _counts.GetValueOrDefault(product.Id);
                yield return new InventoryRow(product.Id, product.Describe(), quantity, product.Price * quantity);
            }
        }

        public Price GrandTotal() => Rows().Aggregate(Price.Zero, (sum, row) => sum + row.Value);
    }

    public sealed class BacklogReport : Report<BacklogRow>
    {
        private readonly IReadOnlyList<Order> _orders;

        public BacklogReport(IReadOnlyList<Order> orders)
            : base("Open orders") => _orders = orders;

        public override string[] Headers => new[] { "Order", "Customer", "Status", "Lines", "Total" };

        protected override IEnumerable<BacklogRow> Rows() =>
            _orders
                .OrderByDescending(o => o.Total.Amount)
                .Select(o => new BacklogRow(o.Id, o.Customer, o.Status, o.Count, o.Total));
    }

    /// <summary>Built-in renderers and a registry of custom ones.</summary>
    public static class Renderers
    {
        private static readonly Dictionary<string, IRenderer> Registry = new(StringComparer.OrdinalIgnoreCase);

        public static readonly IRenderer Text = Register(new TextRenderer());

        public static readonly IRenderer Csv = Register(new CsvRenderer(';'));

        public static IRenderer Register(IRenderer renderer)
        {
            Registry[renderer.Name] = renderer;
            return renderer;
        }

        public static IRenderer Get(string name) =>
            Registry.TryGetValue(name, out var renderer) ? renderer : throw new NotFoundException("renderer", name);

        public static IEnumerable<string> Names => Registry.Keys.OrderBy(k => k);

        private sealed class TextRenderer : IRenderer
        {
            public string Name => "text";

            public void Render(TextWriter output, string title, string[] headers, Table rows)
            {
                var widths = Widths(headers, rows);
                output.WriteLine(title);
                output.WriteLine(new string('=', title.Length));
                WriteRow(output, headers, widths);
                output.WriteLine(string.Join("-+-", widths.Select(w => new string('-', w))));
                foreach (var row in rows)
                {
                    WriteRow(output, row, widths);
                }
                output.WriteLine($"({rows.Count} row{(rows.Count == 1 ? "" : "s")})");

                static void WriteRow(TextWriter output, string[] cells, int[] widths)
                {
                    var padded = cells.Select((cell, i) => Layout.Pad(cell, widths[i], Layout.IsNumeric(cell)));
                    output.WriteLine(string.Join(" | ", padded));
                }
            }

            private static int[] Widths(string[] headers, Table rows)
            {
                var widths = headers.Select(h => h.Length).ToArray();
                foreach (var row in rows)
                {
                    for (var i = 0; i < row.Length && i < widths.Length; i++)
                    {
                        widths[i] = Math.Max(widths[i], row[i].Length);
                    }
                }
                return widths;
            }
        }

        private sealed class CsvRenderer : IRenderer
        {
            private readonly char _separator;

            public CsvRenderer(char separator) => _separator = separator;

            string IRenderer.Name => "csv";

            void IRenderer.Render(TextWriter output, string title, string[] headers, Table rows)
            {
                output.WriteLine(Line(headers));
                rows.ForEach(row => output.WriteLine(Line(row)));
            }

            private string Line(IEnumerable<string> cells) =>
                string.Join(_separator, cells.Select(c => Layout.Quote(c, _separator)));
        }
    }

    /// <summary>Text layout helpers, with nested configuration types.</summary>
    public static class Layout
    {
        public static string Pad(string cell, int width, bool right) =>
            right ? cell.PadLeft(width) : cell.PadRight(width);

        public static bool IsNumeric(string cell) =>
            decimal.TryParse(cell.Split(' ')[0], NumberStyles.Number, CultureInfo.InvariantCulture, out _);

        public static string Quote(string cell, char separator)
        {
            var needs = cell.IndexOfAny(new[] { separator, '"', '\n' }) >= 0;
            return needs ? $"\"{cell.Replace("\"", "\"\"")}\"" : cell;
        }

        public static string Truncate(this string text, int max) =>
            text.Length <= max ? text : string.Concat(text.AsSpan(0, Math.Max(0, max - 1)), "…");

        public sealed class Style
        {
            public int MaxWidth { get; init; } = 32;

            public Palette Colors { get; init; } = Palette.Plain;

            public sealed class Palette
            {
                public static readonly Palette Plain = new("", "");

                public static readonly Palette Ansi = new("\u001b[1m", "\u001b[0m");

                private Palette(string bold, string reset) => (Bold, Reset) = (bold, reset);

                public string Bold { get; }

                public string Reset { get; }

                public string Emphasize(string text) => Bold + text + Reset;
            }
        }
    }

    /// <summary>Entry points used by the command-line front end.</summary>
    public static class ReportBuilder
    {
        public static async Task<InventoryReport> InventoryAsync(ServiceHost host)
        {
            var products = await host.Products.ListAsync();
            var counts = await host.Inventory.CountAsync();
            return new InventoryReport(products, counts);
        }

        public static async Task<BacklogReport> BacklogAsync(OrderRepository orders)
        {
            var open = await orders.OpenAsync();
            return new BacklogReport(open);
        }

        public static string Errors(IEnumerable<WarehouseException> errors)
        {
            var builder = new StringBuilder();
            var grouped = errors
                .GroupBy(e => e.Code)
                .OrderByDescending(g => g.Count());
            foreach (var group in grouped)
            {
                var status = ErrorCatalog.Http.StatusFor(group.Key);
                builder.AppendLine(CultureInfo.InvariantCulture, $"{group.Key} (HTTP {status}): {group.Count()}");
                foreach (var error in group.Take(3))
                {
                    builder.Append("  - ").AppendLine(ErrorCatalog.Explain(error).Truncate(72));
                }
            }
            return builder.Length == 0 ? "no errors" : builder.ToString();
        }

        public static string Render<TRow>(Report<TRow> report, string format)
            where TRow : IRow
        {
            var renderer = Renderers.Get(format);
            return report.RenderWith(renderer);
        }

        public static int Checksum(string rendered)
        {
            unchecked
            {
                var hash = 17;
                foreach (var ch in rendered)
                {
                    hash = hash * 31 + ch;
                }
                return hash;
            }
        }
    }

    /// <summary>Summary shown on the dashboard.</summary>
    public readonly struct Summary
    {
        public Summary(int products, int openOrders, Price stockValue)
        {
            Products = products;
            OpenOrders = openOrders;
            StockValue = stockValue;
        }

        public int Products { get; }

        public int OpenOrders { get; }

        public Price StockValue { get; }

        public static async Task<Summary> ComputeAsync(ServiceHost host)
        {
            var inventory = await ReportBuilder.InventoryAsync(host);
            var open = await host.Orders.OpenAsync();
            return new Summary(host.Products.Count, open.Count, inventory.GrandTotal());
        }

        public static explicit operator string(Summary s) =>
            $"{s.Products} products, {s.OpenOrders} open orders, stock worth {s.StockValue}";
    }
}

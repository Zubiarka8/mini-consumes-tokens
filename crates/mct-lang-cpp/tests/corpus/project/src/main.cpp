// main.cpp — command-line front end of the inventory system.
//
//   inventory [--data DIR] <command> [args...]
//
// Commands: demo, receive, ship, transfer, stock, reorder, export, help.

#include "inventory/model.hpp"
#include "inventory/storage.hpp"

#include <cstdlib>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

// Forward declarations of what src/service.cpp provides.
namespace inv::service {
inline namespace v2 {
class InventoryService;
struct Suggestion;
}  // namespace v2
std::unique_ptr<InventoryService> make_service(ProductRepository& products, StockRepository& stock,
                                               FileStore* journal);
void seed_demo(InventoryService& service, ProductRepository& products);
double average_stock(const InventoryService& service, ProductId product, std::size_t locations);
}  // namespace inv::service

std::string inv_version_string();

namespace cli {

using Args = std::vector<std::string>;

/// Exit codes, sysexits-style.
enum ExitCode : int {
    kOk = 0,
    kFailure = 1,
    kUsage = 64,
    kDataError = 65,
    kSoftware = 70,
};

/// Global options parsed before the command name.
struct Options {
    std::string data_dir = ".inventory";
    bool verbose = false;
    bool json = false;
    std::string command;
    Args args;
};

/// Everything a command needs, built once in main().
struct Context {
    Options options;
    std::unique_ptr<inv::ProductRepository> products;
    inv::StockRepository stock{"stock"};
    std::unique_ptr<inv::FileStore> journal;
    std::unique_ptr<inv::service::InventoryService> service;
    std::ostream& out = std::cout;
    std::ostream& err = std::cerr;
};

using Handler = int (*)(Context&);

class UsageError : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

void print_usage(std::ostream& out) {
    out << "inventory " << inv_version_string() << "\n\n"
        << "usage: inventory [--data DIR] [--verbose] [--json] <command> [args]\n\n"
        << "commands:\n"
        << "  demo                              seed demo data\n"
        << "  receive  PRODUCT LOC AMOUNT SUPPLIER\n"
        << "  ship     PRODUCT LOC AMOUNT ORDER\n"
        << "  transfer PRODUCT FROM TO AMOUNT\n"
        << "  stock    PRODUCT\n"
        << "  reorder                           suggest purchase orders\n"
        << "  export                            dump the journal as CSV\n"
        << "  help                              this text\n";
}

Options parse_options(int argc, char** argv) {
    Options options;
    int i = 1;
    for (; i < argc; ++i) {
        const std::string arg = argv[i];
        if (arg == "--data") {
            if (i + 1 >= argc) throw UsageError("--data needs a directory");
            options.data_dir = argv[++i];
        } else if (arg.rfind("--data=", 0) == 0) {
            options.data_dir = arg.substr(7);
        } else if (arg == "-v" || arg == "--verbose") {
            options.verbose = true;
        } else if (arg == "--json") {
            options.json = true;
        } else if (!arg.empty() && arg[0] == '-') {
            throw UsageError("unknown option " + arg);
        } else {
            break;
        }
    }
    if (i >= argc) throw UsageError("missing command");
    options.command = argv[i++];
    for (; i < argc; ++i) options.args.emplace_back(argv[i]);
    return options;
}

/// Parses "W/A/S" into a location.
inv::Location parse_location(const std::string& text) {
    inv::Location where;
    char slash1 = 0;
    char slash2 = 0;
    std::istringstream in(text);
    unsigned warehouse = 0;
    unsigned aisle = 0;
    unsigned shelf = 0;
    if (!(in >> warehouse >> slash1 >> aisle >> slash2 >> shelf) || slash1 != '/' || slash2 != '/') {
        throw UsageError("bad location '" + text + "', expected W/A/S");
    }
    where.warehouse = warehouse;
    where.aisle = static_cast<std::uint16_t>(aisle);
    where.shelf = static_cast<std::uint16_t>(shelf);
    return where;
}

inv::ProductId parse_product(const std::string& text) {
    const auto digits = text.rfind("SKU-", 0) == 0 ? text.substr(4) : text;
    char* end = nullptr;
    const auto value = std::strtoull(digits.c_str(), &end, 10);
    if (end == digits.c_str() || *end != '\0') throw UsageError("bad product id '" + text + "'");
    return inv::ProductId(value);
}

std::int64_t parse_amount(const std::string& text) {
    try {
        std::size_t used = 0;
        const auto value = std::stoll(text, &used);
        if (used != text.size() || value <= 0) throw UsageError("amount must be a positive integer");
        return value;
    } catch (const std::logic_error&) {
        throw UsageError("bad amount '" + text + "'");
    }
}

void require_args(const Context& ctx, std::size_t n) {
    if (ctx.options.args.size() != n) {
        throw UsageError(ctx.options.command + " takes " + std::to_string(n) + " argument(s)");
    }
}

void print_level(Context& ctx, const inv::StockLevel& level) {
    if (ctx.options.json) {
        ctx.out << "{\"product\":" << level.product.value() << ",\"location\":\"" << level.location.label()
                << "\",\"on_hand\":" << level.on_hand.amount() << ",\"reserved\":" << level.reserved.amount()
                << "}\n";
    } else {
        ctx.out << level.product << " @ " << level.location.label() << ": " << level.on_hand.to_string()
                << " (" << level.reserved.amount() << " reserved)\n";
    }
}

int cmd_demo(Context& ctx) {
    inv::service::seed_demo(*ctx.service, *ctx.products);
    ctx.out << "seeded " << ctx.products->size() << " products\n";
    return kOk;
}

int cmd_receive(Context& ctx) {
    require_args(ctx, 4);
    const auto& a = ctx.options.args;
    auto result = ctx.service->receive(parse_product(a[0]), parse_location(a[1]), parse_amount(a[2]), a[3]);
    if (!result.has_value()) {
        ctx.err << "error: " << result.error() << '\n';
        return kDataError;
    }
    print_level(ctx, result.value());
    return kOk;
}

int cmd_ship(Context& ctx) {
    require_args(ctx, 4);
    const auto& a = ctx.options.args;
    auto result = ctx.service->ship(parse_product(a[0]), parse_location(a[1]), parse_amount(a[2]), a[3]);
    if (!result.has_value()) {
        ctx.err << "error: " << result.error() << '\n';
        return kDataError;
    }
    print_level(ctx, result.value());
    return kOk;
}

int cmd_transfer(Context& ctx) {
    require_args(ctx, 4);
    const auto& a = ctx.options.args;
    auto result =
        ctx.service->transfer(parse_product(a[0]), parse_location(a[1]), parse_location(a[2]), parse_amount(a[3]));
    if (!result.has_value()) {
        ctx.err << "error: " << result.error() << '\n';
        return kDataError;
    }
    const auto& [from, to] = result.value();
    print_level(ctx, from);
    print_level(ctx, to);
    return kOk;
}

int cmd_stock(Context& ctx) {
    require_args(ctx, 1);
    const auto product = parse_product(ctx.options.args[0]);
    const auto& described = ctx.products->require(product);
    ctx.out << described.describe() << '\n';
    std::size_t locations = 0;
    ctx.stock.for_each([&](const inv::StockKey& key, const inv::StockLevel& level) {
        if (key.product == product) {
            print_level(ctx, level);
            ++locations;
        }
    });
    ctx.out << "total: " << ctx.service->total_on_hand(product)
            << ", average per location: " << inv::service::average_stock(*ctx.service, product, locations) << '\n';
    return kOk;
}

int cmd_reorder(Context& ctx) {
    const auto suggestions = ctx.service->reorder_suggestions();
    if (suggestions.empty()) {
        ctx.out << "nothing to reorder\n";
        return kOk;
    }
    for (const auto& s : suggestions) {
        ctx.out << inv::make_sku(s.product) << "\t" << s.quantity << "\t" << s.reason << '\n';
    }
    return kOk;
}

int cmd_export(Context& ctx) {
    if (!ctx.journal) {
        ctx.err << "no journal configured\n";
        return kFailure;
    }
    ctx.out << "product,location,kind,delta\n";
    for (const auto& movement : ctx.journal->load_all()) {
        ctx.out << movement.product.value() << ',' << movement.where.label() << ',' << movement.kind_name() << ','
                << movement.delta() << '\n';
    }
    return kOk;
}

int cmd_help(Context& ctx) {
    print_usage(ctx.out);
    return kOk;
}

const std::map<std::string, Handler>& handlers() {
    static const std::map<std::string, Handler> table{
        {"demo", &cmd_demo},         {"receive", &cmd_receive}, {"ship", &cmd_ship},
        {"transfer", &cmd_transfer}, {"stock", &cmd_stock},     {"reorder", &cmd_reorder},
        {"export", &cmd_export},     {"help", &cmd_help},
    };
    return table;
}

Context make_context(Options options) {
    Context ctx;
    ctx.options = std::move(options);
    ctx.products = inv::make_product_repository("memory");
    ctx.journal = std::make_unique<inv::FileStore>(inv::FileStore::Options{ctx.options.data_dir + "/journal"});
    ctx.service = inv::service::make_service(*ctx.products, ctx.stock, ctx.journal.get());
    return ctx;
}

int run(int argc, char** argv) {
    Options options;
    try {
        options = parse_options(argc, argv);
    } catch (const UsageError& e) {
        std::cerr << "usage error: " << e.what() << "\n\n";
        print_usage(std::cerr);
        return kUsage;
    }
    const auto it = handlers().find(options.command);
    if (it == handlers().end()) {
        std::cerr << "unknown command '" << options.command << "'\n";
        return kUsage;
    }
    auto ctx = make_context(std::move(options));
    try {
        return it->second(ctx);
    } catch (const UsageError& e) {
        ctx.err << "usage error: " << e.what() << '\n';
        return kUsage;
    } catch (const inv::StorageError& e) {
        ctx.err << "storage error (" << e.code() << "): " << e.what() << '\n';
        return kDataError;
    } catch (const std::exception& e) {
        ctx.err << "internal error: " << e.what() << '\n';
        return kSoftware;
    }
}

}  // namespace cli

int main(int argc, char** argv) {
    return cli::run(argc, argv);
}

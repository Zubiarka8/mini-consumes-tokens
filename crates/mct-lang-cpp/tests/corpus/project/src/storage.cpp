// storage.cpp — out-of-line definitions for storage.hpp and model.hpp:
// the file-backed movement journal, snapshots and the repository factory.

#include "inventory/storage.hpp"
#include "inventory/model.hpp"

#include <cstdio>
#include <fstream>
#include <iomanip>
#include <sstream>
#include <system_error>

namespace {

// Field separator of the journal format; a tab never appears in names.
constexpr char kSep = '\t';

std::vector<std::string_view> split(std::string_view line, char sep) {
    std::vector<std::string_view> parts;
    std::size_t start = 0;
    while (true) {
        const auto pos = line.find(sep, start);
        parts.push_back(line.substr(start, pos - start));
        if (pos == std::string_view::npos) break;
        start = pos + 1;
    }
    return parts;
}

std::int64_t to_int(std::string_view text, std::int64_t fallback = 0) {
    std::int64_t value = 0;
    bool negative = false;
    std::size_t i = 0;
    if (!text.empty() && text[0] == '-') {
        negative = true;
        i = 1;
    }
    if (i == text.size()) return fallback;
    for (; i < text.size(); ++i) {
        if (text[i] < '0' || text[i] > '9') return fallback;
        value = value * 10 + (text[i] - '0');
    }
    return negative ? -value : value;
}

std::string encode_location(const inv::Location& where) {
    std::ostringstream out;
    out << where.warehouse << '/' << where.aisle << '/' << where.shelf;
    return out.str();
}

inv::Location decode_location(std::string_view text) {
    const auto parts = split(text, '/');
    inv::Location where;
    if (parts.size() == 3) {
        where.warehouse = static_cast<inv::WarehouseId>(to_int(parts[0]));
        where.aisle = static_cast<std::uint16_t>(to_int(parts[1]));
        where.shelf = static_cast<std::uint16_t>(to_int(parts[2]));
    }
    return where;
}

}  // namespace

namespace inv {

// ---------------------------------------------------------------------------
// model.hpp out-of-line members
// ---------------------------------------------------------------------------

template <typename T>
void Quantity<T>::throw_unit_mismatch(Unit a, Unit b) {
    throw std::invalid_argument(std::string("unit mismatch: ") + unit_symbol(a) + " vs " + unit_symbol(b));
}


Product::Product(ProductId id, std::string name, Unit unit)
    : id_(id), name_(detail::normalize_name(name)), unit_(unit) {
    const auto problem = detail::validate_name(name_);
    if (!problem.empty()) throw std::invalid_argument(problem);
}

bool Product::has_tag(std::string_view tag) const {
    return std::any_of(tags_.begin(), tags_.end(), [tag](const std::string& t) { return t == tag; });
}

std::string Product::describe() const {
    std::string text = sku() + " " + name_ + " [" + unit_symbol(unit_) + "]";
    if (dims_) text += " " + std::to_string(dims_->volume()) + " cm3";
    return text;
}

Product Product::Builder::build() const {
    Product product(id_, name_, unit_);
    product.tags_ = tags_;
    product.dims_ = dims_;
    return product;
}

std::string PerishableProduct::describe() const {
    auto text = Product::describe();
    return text + (expired() ? " (EXPIRED)" : " (fresh)");
}

namespace detail {

std::string normalize_name(std::string_view raw) {
    std::string out;
    bool pending_space = false;
    for (char c : raw) {
        if (c == ' ' || c == '\t' || c == '\n') {
            pending_space = !out.empty();
            continue;
        }
        if (pending_space) out.push_back(' ');
        pending_space = false;
        out.push_back(c);
    }
    return out;
}

std::string validate_name(std::string_view name) {
    if (name.empty()) return "product name must not be empty";
    if (name.size() > kMaxNameLength) return "product name too long";
    if (name.find(kSep) != std::string_view::npos) return "product name must not contain tabs";
    return {};
}

}  // namespace detail

// ---------------------------------------------------------------------------
// storage.hpp out-of-line members
// ---------------------------------------------------------------------------

std::string Conflict::format_conflict(std::string_view key, std::uint64_t expected, std::uint64_t actual) {
    std::ostringstream out;
    out << "conflict on " << key << ": expected v" << expected << ", found v" << actual;
    return out.str();
}

/// Buffered, optionally fsync-ing line writer (the FileStore pimpl).
class FileStore::Writer {
public:
    Writer(const std::filesystem::path& path, bool fsync) : out_(path, std::ios::app), fsync_(fsync) {
        if (!out_) throw StorageError("cannot open " + path.string());
    }

    std::size_t write_line(const std::string& line) {
        out_ << line << '\n';
        if (fsync_) out_.flush();
        if (!out_) throw StorageError("write failed");
        return line.size() + 1;
    }

private:
    std::ofstream out_;
    bool fsync_;
};

FileStore::FileStore(Options options) : options_(std::move(options)) {
    std::error_code ec;
    std::filesystem::create_directories(options_.directory, ec);
    if (ec) throw StorageError("cannot create " + options_.directory.string() + ": " + ec.message());
    current_ = next_path();
    writer_ = std::make_unique<Writer>(current_, options_.fsync);
}

FileStore::~FileStore() = default;
FileStore::FileStore(FileStore&&) noexcept = default;
FileStore& FileStore::operator=(FileStore&&) noexcept = default;

std::filesystem::path FileStore::next_path() const {
    const auto stamp = std::chrono::duration_cast<std::chrono::seconds>(Clock::now().time_since_epoch()).count();
    return options_.directory / ("journal-" + std::to_string(stamp) + ".log");
}

void FileStore::append(const Movement& movement) {
    std::lock_guard lock(mutex_);
    bytes_written_ += writer_->write_line(encode(movement));
    rotate_if_needed();
}

std::size_t FileStore::rotate_if_needed() {
    if (bytes_written_ < options_.max_file_bytes) return 0;
    current_ = next_path();
    writer_ = std::make_unique<Writer>(current_, options_.fsync);
    const auto previous = bytes_written_;
    bytes_written_ = 0;
    return previous;
}

std::vector<Movement> FileStore::load_all() const {
    std::lock_guard lock(mutex_);
    std::vector<Movement> movements;
    for (const auto& entry : std::filesystem::directory_iterator(options_.directory)) {
        if (entry.path().extension() != ".log") continue;
        std::ifstream in(entry.path());
        std::string line;
        while (std::getline(in, line)) {
            if (auto movement = decode(line)) movements.push_back(*std::move(movement));
        }
    }
    std::sort(movements.begin(), movements.end(), [](const Movement& a, const Movement& b) { return a.at < b.at; });
    return movements;
}

void FileStore::compact(const std::function<bool(const Movement&)>& keep) {
    auto kept = load_all();
    kept.erase(std::remove_if(kept.begin(), kept.end(), [&keep](const Movement& m) { return !keep(m); }), kept.end());
    for (const auto& entry : std::filesystem::directory_iterator(options_.directory)) {
        std::filesystem::remove(entry.path());
    }
    current_ = next_path();
    writer_ = std::make_unique<Writer>(current_, options_.fsync);
    bytes_written_ = 0;
    for (const auto& movement : kept) append(movement);
}

std::string FileStore::encode(const Movement& movement) {
    std::ostringstream out;
    out << movement.product.value() << kSep << encode_location(movement.where) << kSep << movement.kind_name()
        << kSep << movement.delta() << kSep
        << std::chrono::duration_cast<std::chrono::seconds>(movement.at.time_since_epoch()).count();
    return out.str();
}

std::optional<Movement> FileStore::decode(std::string_view line) {
    const auto fields = split(line, kSep);
    if (fields.size() != 5) return std::nullopt;
    Movement movement;
    movement.product = ProductId(static_cast<std::uint64_t>(to_int(fields[0])));
    movement.where = decode_location(fields[1]);
    const auto amount = to_int(fields[3]);
    if (fields[2] == "receipt") {
        movement.kind = Receipt{Count(amount, Unit::Piece), "journal"};
    } else if (fields[2] == "shipment") {
        movement.kind = Shipment{Count(-amount, Unit::Piece), "journal"};
    } else {
        movement.kind = Adjustment{Count(amount < 0 ? -amount : amount, Unit::Piece), amount >= 0, "journal"};
    }
    movement.at = Timestamp(std::chrono::seconds(to_int(fields[4])));
    return movement;
}

// ---------------------------------------------------------------------------
// Snapshots and factory
// ---------------------------------------------------------------------------

Snapshot take_snapshot(const ProductRepository& products, const StockRepository& stock) {
    Snapshot snapshot;
    snapshot.products = products.values();
    snapshot.levels = stock.values();
    return snapshot;
}

void restore_snapshot(const Snapshot& snapshot, ProductRepository& products, StockRepository& stock) {
    std::vector<ProductId> existing;
    products.for_each([&existing](const ProductId& id, const Product&) { existing.push_back(id); });
    for (const auto& id : existing) products.erase(id);
    for (const auto& product : snapshot.products) products.put(product.id(), product);
    stock.clear();
    for (const auto& level : snapshot.levels) stock.put(StockKey{level.product, level.location}, level);
}

std::unique_ptr<ProductRepository> make_product_repository(std::string_view kind) {
    if (kind == "memory") return std::make_unique<MemoryRepository<ProductId, Product>>("product");
    throw StorageError("unknown repository kind: " + std::string(kind));
}

}  // namespace inv

// Qualified definitions outside any namespace block.
std::string inv::Location::label() const {
    std::ostringstream out;
    out << 'W' << warehouse << '-' << std::setw(2) << std::setfill('0') << aisle << '-' << shelf;
    return out.str();
}

const char* inv::StoreHealth::status_name() const noexcept {
    switch (status_) {
    case Status::Ok:
        return "ok";
    case Status::Degraded:
        return "degraded";
    case Status::Failed:
        return "failed";
    }
    return "?";
}

void inv::StoreHealth::degrade(std::string reason) {
    if (status_ == Status::Ok) status_ = Status::Degraded;
    reasons_.push_back(std::move(reason));
}

std::string inv::describe_error(const StorageError& error) {
    return "[" + std::to_string(error.code()) + "] " + error.what();
}

std::string inv_version_string() {
    return "inventory/" + std::to_string(inv::detail::kSchemaVersion);
}

// storage.hpp — persistence interfaces and their in-memory and file-backed
// implementations. MemoryRepository is a header-only template; FileStore is
// declared here and defined in src/storage.cpp.
#pragma once

#include "inventory/model.hpp"

#include <filesystem>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <shared_mutex>
#include <stdexcept>
#include <unordered_map>

namespace inv {

/// Base of every storage failure.
class StorageError : public std::runtime_error {
public:
    explicit StorageError(const std::string& what) : std::runtime_error(what) {}
    virtual int code() const noexcept { return 1; }
};

class NotFound : public StorageError {
public:
    NotFound(std::string_view kind, std::string_view key)
        : StorageError(std::string(kind) + " not found: " + std::string(key)) {}
    int code() const noexcept override { return 404; }
};

class Conflict : public StorageError {
public:
    Conflict(std::string_view key, std::uint64_t expected, std::uint64_t actual)
        : StorageError(format_conflict(key, expected, actual)) {}
    int code() const noexcept override { return 409; }

private:
    static std::string format_conflict(std::string_view key, std::uint64_t expected, std::uint64_t actual);
};

/// Keyed store of values of one type.
template <typename K, typename V>
class Repository {
public:
    using key_type = K;
    using mapped_type = V;
    using Visitor = std::function<void(const K&, const V&)>;

    virtual ~Repository() = default;

    virtual std::optional<V> find(const K& key) const = 0;
    virtual void put(const K& key, V value) = 0;
    virtual bool erase(const K& key) = 0;
    virtual void for_each(const Visitor& visit) const = 0;
    virtual std::size_t size() const = 0;

    /// Like find(), but throws NotFound.
    V require(const K& key) const {
        auto value = find(key);
        if (!value) throw NotFound(kind(), key_to_string(key));
        return *std::move(value);
    }

    bool contains(const K& key) const { return find(key).has_value(); }

    std::vector<V> values() const {
        std::vector<V> out;
        out.reserve(size());
        for_each([&out](const K&, const V& v) { out.push_back(v); });
        return out;
    }

protected:
    virtual std::string_view kind() const { return "entity"; }

    static std::string key_to_string(const K& key) {
        if constexpr (std::is_arithmetic_v<K>) {
            return std::to_string(key);
        } else if constexpr (std::is_same_v<K, ProductId>) {
            return make_sku(key);
        } else {
            return std::string(key);
        }
    }
};

/// Thread-safe, versioned, dictionary-backed repository.
template <typename K, typename V>
class MemoryRepository : public Repository<K, V> {
public:
    using Visitor = typename Repository<K, V>::Visitor;

    explicit MemoryRepository(std::string kind = "entity") : kind_(std::move(kind)) {}

    std::optional<V> find(const K& key) const override {
        std::shared_lock lock(mutex_);
        auto it = data_.find(key);
        if (it == data_.end()) return std::nullopt;
        return it->second.value;
    }

    void put(const K& key, V value) override {
        std::unique_lock lock(mutex_);
        auto& slot = data_[key];
        slot.value = std::move(value);
        ++slot.version;
    }

    /// Optimistic write: fails with Conflict if the version moved.
    std::uint64_t put_if_version(const K& key, V value, std::uint64_t expected) {
        std::unique_lock lock(mutex_);
        auto& slot = data_[key];
        if (slot.version != expected) {
            throw Conflict(Repository<K, V>::key_to_string(key), expected, slot.version);
        }
        slot.value = std::move(value);
        return ++slot.version;
    }

    std::uint64_t version(const K& key) const;

    bool erase(const K& key) override {
        std::unique_lock lock(mutex_);
        return data_.erase(key) > 0;
    }

    void for_each(const Visitor& visit) const override {
        std::shared_lock lock(mutex_);
        for (const auto& [key, slot] : data_) visit(key, slot.value);
    }

    std::size_t size() const override {
        std::shared_lock lock(mutex_);
        return data_.size();
    }

    void clear() {
        std::unique_lock lock(mutex_);
        data_.clear();
    }

protected:
    std::string_view kind() const override { return kind_; }

private:
    struct Slot {
        V value{};
        std::uint64_t version = 0;
    };

    std::string kind_;
    mutable std::shared_mutex mutex_;
    std::map<K, Slot> data_;
};

// Out-of-line member of a class template, defined in the header.
template <typename K, typename V>
std::uint64_t MemoryRepository<K, V>::version(const K& key) const {
    std::shared_lock lock(mutex_);
    auto it = data_.find(key);
    return it == data_.end() ? 0 : it->second.version;
}

/// Stock level of one product at one location.
struct StockLevel {
    ProductId product;
    Location location;
    Count on_hand;
    Count reserved;

    Count available() const { return on_hand - reserved; }
};

/// Composite key for stock levels.
struct StockKey {
    ProductId product;
    Location location;

    bool operator<(const StockKey& other) const {
        if (product < other.product) return true;
        if (other.product < product) return false;
        return location < other.location;
    }
};

using ProductRepository = Repository<ProductId, Product>;
using StockRepository = MemoryRepository<StockKey, StockLevel>;

/// Append-only movement journal persisted as one line per movement.
class FileStore {
public:
    struct Options {
        std::filesystem::path directory;
        bool fsync = true;
        std::size_t max_file_bytes = 16 * 1024 * 1024;
    };

    explicit FileStore(Options options);
    ~FileStore();

    FileStore(const FileStore&) = delete;
    FileStore& operator=(const FileStore&) = delete;
    FileStore(FileStore&&) noexcept;
    FileStore& operator=(FileStore&&) noexcept;

    void append(const Movement& movement);
    std::vector<Movement> load_all() const;
    std::size_t rotate_if_needed();
    void compact(const std::function<bool(const Movement&)>& keep);

    const std::filesystem::path& path() const { return current_; }
    std::size_t bytes_written() const noexcept { return bytes_written_; }

    static std::string encode(const Movement& movement);
    static std::optional<Movement> decode(std::string_view line);

private:
    class Writer;  // pimpl, defined in storage.cpp

    std::filesystem::path next_path() const;

    Options options_;
    std::filesystem::path current_;
    std::unique_ptr<Writer> writer_;
    std::size_t bytes_written_ = 0;
    mutable std::mutex mutex_;
};

/// Snapshot of every repository, for backups and tests.
struct Snapshot {
    std::vector<Product> products;
    std::vector<StockLevel> levels;
    Timestamp taken = Clock::now();

    std::size_t item_count() const { return products.size() + levels.size(); }
};

Snapshot take_snapshot(const ProductRepository& products, const StockRepository& stock);
void restore_snapshot(const Snapshot& snapshot, ProductRepository& products, StockRepository& stock);

/// Factory returning the repository implementation named by `kind`.
std::unique_ptr<ProductRepository> make_product_repository(std::string_view kind);

/// RAII guard that restores a repository from a snapshot unless committed.
class Transaction {
public:
    Transaction(ProductRepository& products, StockRepository& stock)
        : products_(products), stock_(stock), before_(take_snapshot(products, stock)) {}

    ~Transaction() {
        if (!committed_) rollback();
    }

    void commit() noexcept { committed_ = true; }
    void rollback() { restore_snapshot(before_, products_, stock_); }

private:
    ProductRepository& products_;
    StockRepository& stock_;
    Snapshot before_;
    bool committed_ = false;
};

/// Health of a store, reported by `inventory doctor`.
class StoreHealth {
public:
    enum class Status { Ok, Degraded, Failed };

    struct Flags {
        unsigned readonly : 1;
        unsigned rotating : 1;
        unsigned reserved : 6;
    };

    Status status() const noexcept { return status_; }
    const char* status_name() const noexcept;
    void degrade(std::string reason);

    friend bool operator==(const StoreHealth& a, const StoreHealth& b) { return a.status_ == b.status_; }

private:
    Status status_ = Status::Ok;
    Flags flags_{};
    std::vector<std::string> reasons_;
};

static_assert(sizeof(StoreHealth::Flags) <= sizeof(unsigned), "flags must fit in one word");

std::string describe_error(const StorageError& error);

}  // namespace inv

namespace std {

/// Hash specialisation so StockKey works in unordered containers.
template <>
struct hash<inv::StockKey> {
    std::size_t operator()(const inv::StockKey& key) const noexcept {
        const auto h1 = std::hash<std::uint64_t>{}(key.product.value());
        const auto h2 = std::hash<std::uint32_t>{}(key.location.warehouse);
        return h1 ^ (h2 << 1);
    }
};

}  // namespace std

// C++ tests for the diplomat-generated mlt-ffi shared library.
// Validates the FFI contract from a real C/C++ consumer's perspective:
// header correctness, memory lifecycle, and encoding data integrity.

#include <cstdint>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

#include <gtest/gtest.h>

// Diplomat-generated headers (C ABI)
extern "C" {
#include "MltEncoderConfig.h"
#include "MltEncodedBuffer.h"
#include "MltError.h"
#include "MltLayerEncoder.h"
#include "MltTileEncoder.h"
#include "diplomat_runtime.h"
}

// ── RAII wrappers for diplomat opaque handles ──────────────────────────

struct EncoderConfigDeleter {
    void operator()(MltEncoderConfig* p) const { MltEncoderConfig_destroy(p); }
};
using UniqueConfig = std::unique_ptr<MltEncoderConfig, EncoderConfigDeleter>;

struct LayerEncoderDeleter {
    void operator()(MltLayerEncoder* p) const { MltLayerEncoder_destroy(p); }
};
using UniqueLayerEncoder = std::unique_ptr<MltLayerEncoder, LayerEncoderDeleter>;

struct TileEncoderDeleter {
    void operator()(MltTileEncoder* p) const { MltTileEncoder_destroy(p); }
};
using UniqueTileEncoder = std::unique_ptr<MltTileEncoder, TileEncoderDeleter>;

struct EncodedBufferDeleter {
    void operator()(MltEncodedBuffer* p) const { MltEncodedBuffer_destroy(p); }
};
using UniqueBuffer = std::unique_ptr<MltEncodedBuffer, EncodedBufferDeleter>;

struct ErrorDeleter {
    void operator()(MltError* p) const { MltError_destroy(p); }
};
using UniqueError = std::unique_ptr<MltError, ErrorDeleter>;

// ── Helpers ─────────────────────────────────────────────────────────────

static DiplomatStringView str_view(const char* s) {
    return {s, std::strlen(s)};
}

template <typename T>
static auto make_view(const std::vector<T>& v) {
    // All diplomat view types have the same {data, len} layout.
    struct {
        const T* data;
        size_t len;
    } view = {v.data(), v.size()};
    return view;
}

static DiplomatStringView string_view(const std::string& s) {
    return {s.data(), s.size()};
}

static DiplomatU8View u8_view(const std::vector<uint8_t>& v) {
    return {v.data(), v.size()};
}

static DiplomatU8View u8_view_raw(const void* data, size_t len) {
    return {static_cast<const uint8_t*>(data), len};
}

static DiplomatI32View i32_view(const std::vector<int32_t>& v) {
    return {v.data(), v.size()};
}

static DiplomatU32View u32_view(const std::vector<uint32_t>& v) {
    return {v.data(), v.size()};
}

static DiplomatU64View u64_view(const std::vector<uint64_t>& v) {
    return {v.data(), v.size()};
}

static DiplomatBoolView bool_view(const std::vector<bool>&) = delete;

// bool views need special handling since vector<bool> is packed
struct BoolVec {
    std::vector<uint8_t> data;
    explicit BoolVec(size_t n, bool val = false)
        : data(n, val ? 1 : 0) {}
    void set(size_t i, bool v) { data[i] = v ? 1 : 0; }
    [[nodiscard]] DiplomatBoolView view() const { return {reinterpret_cast<const bool*>(data.data()), data.size()}; }
    [[nodiscard]] DiplomatBoolView empty_view() const { return {nullptr, 0}; }
};

static DiplomatBoolView empty_bool_view() {
    return {nullptr, 0};
}
static DiplomatU32View empty_u32_view() {
    return {nullptr, 0};
}
static DiplomatI32View empty_i32_view() {
    return {nullptr, 0};
}

static std::string read_error(MltError* err) {
    auto* write = diplomat_buffer_write_create(256);
    MltError_message(err, write);
    size_t len = diplomat_buffer_write_len(write);
    char* bytes = diplomat_buffer_write_get_bytes(write);
    std::string msg(bytes, len);
    diplomat_buffer_write_destroy(write);
    return msg;
}

// =========================================================================
// Basic encoding tests
// =========================================================================

TEST(FfiEncode, SingleLayerPointsWithProperties) {
    auto result = MltLayerEncoder_new(str_view("test"), 4096);
    ASSERT_TRUE(result.is_ok) << "MltLayerEncoder_new failed";
    UniqueLayerEncoder enc(result.ok);

    // IDs
    std::vector<uint64_t> ids = {1, 2, 3};
    auto id_result = MltLayerEncoder_set_ids(enc.get(), u64_view(ids), empty_bool_view());
    ASSERT_TRUE(id_result.is_ok);

    // Geometry: 3 points
    std::vector<uint8_t> types = {0, 0, 0};
    std::vector<int32_t> vertices = {10, 20, 30, 40, 50, 60};
    auto geo_result = MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(vertices), empty_u32_view(), empty_u32_view(), empty_u32_view());
    ASSERT_TRUE(geo_result.is_ok);

    // i32 column via unified add_column
    std::vector<int32_t> pop = {100, 200, 300};
    auto pop_r = MltLayerEncoder_add_column(
        enc.get(), str_view("pop"), MltColumnType_I32,
        u8_view_raw(pop.data(), pop.size() * sizeof(int32_t)),
        static_cast<uint32_t>(pop.size()), empty_bool_view());
    ASSERT_TRUE(pop_r.is_ok);

    // String column: "alpha", null, "gamma"
    const char* str_data = "alphagamma";
    DiplomatStringView data_view = {str_data, 10};
    std::vector<uint32_t> offsets = {0, 5, 5, 10};
    BoolVec str_present(3, true);
    str_present.set(1, false);
    auto str_r = MltLayerEncoder_add_string_column(
        enc.get(), str_view("name"), data_view, u32_view(offsets), str_present.view());
    ASSERT_TRUE(str_r.is_ok);

    // Encode
    UniqueConfig config(MltEncoderConfig_new_default());
    auto encode_result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(encode_result.is_ok) << "encode failed";
    UniqueBuffer buf(encode_result.ok);

    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
    EXPECT_FALSE(MltEncodedBuffer_is_empty(buf.get()));

    auto bytes_view = MltEncodedBuffer_as_bytes(buf.get());
    EXPECT_EQ(bytes_view.len, MltEncodedBuffer_len(buf.get()));
    EXPECT_NE(bytes_view.data, nullptr);
}

TEST(FfiEncode, TileMultiLayer) {
    UniqueTileEncoder tile(MltTileEncoder_new());
    ASSERT_NE(tile, nullptr);

    // Layer 1
    {
        auto r = MltLayerEncoder_new(str_view("a"), 4096);
        ASSERT_TRUE(r.is_ok);
        auto* enc = r.ok;
        std::vector<uint8_t> types = {0};
        std::vector<int32_t> verts = {1, 2};
        auto gr = MltLayerEncoder_set_geometry(
            enc, u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
        ASSERT_TRUE(gr.is_ok);
        auto ar = MltTileEncoder_add_layer(tile.get(), enc);
        ASSERT_TRUE(ar.is_ok);
    }

    // Layer 2
    {
        auto r = MltLayerEncoder_new(str_view("b"), 4096);
        ASSERT_TRUE(r.is_ok);
        auto* enc = r.ok;
        std::vector<uint8_t> types = {0};
        std::vector<int32_t> verts = {3, 4};
        auto gr = MltLayerEncoder_set_geometry(
            enc, u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
        ASSERT_TRUE(gr.is_ok);
        auto ar = MltTileEncoder_add_layer(tile.get(), enc);
        ASSERT_TRUE(ar.is_ok);
    }

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltTileEncoder_encode(tile.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);

    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

TEST(FfiEncode, PresenceBitmap) {
    auto r = MltLayerEncoder_new(str_view("test"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    // IDs with presence bitmap
    std::vector<uint64_t> ids = {10, 20, 30};
    BoolVec pres(3, true);
    pres.set(1, false); // ID 20 is null
    auto id_r = MltLayerEncoder_set_ids(enc.get(), u64_view(ids), pres.view());
    ASSERT_TRUE(id_r.is_ok);

    // Geometry
    std::vector<uint8_t> types = {0, 0, 0};
    std::vector<int32_t> verts = {1, 2, 3, 4, 5, 6};
    auto geo_r = MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
    ASSERT_TRUE(geo_r.is_ok);

    // Column with presence bitmap via unified add_column
    std::vector<int32_t> vals = {100, 200, 300};
    BoolVec col_pres(3, true);
    col_pres.set(2, false); // value 300 is null
    auto col_r = MltLayerEncoder_add_column(
        enc.get(), str_view("pop"), MltColumnType_I32,
        u8_view_raw(vals.data(), vals.size() * sizeof(int32_t)),
        static_cast<uint32_t>(vals.size()), col_pres.view());
    ASSERT_TRUE(col_r.is_ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);
    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

TEST(FfiEncode, AllColumnTypes) {
    auto r = MltLayerEncoder_new(str_view("types"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    std::vector<uint8_t> types = {0, 0};
    std::vector<int32_t> verts = {1, 2, 3, 4};
    MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());

    // One column of each type via unified add_column
    BoolVec bools(2);
    bools.set(0, true);
    bools.set(1, false);
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("b"), MltColumnType_Bool,
        u8_view_raw(bools.data.data(), bools.data.size()), 2, empty_bool_view()).is_ok);

    std::vector<int8_t> i8s = {-1, 2};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("i8"), MltColumnType_I8,
        u8_view_raw(i8s.data(), i8s.size() * sizeof(int8_t)), 2, empty_bool_view()).is_ok);

    std::vector<uint8_t> u8s = {1, 2};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("u8"), MltColumnType_U8,
        u8_view_raw(u8s.data(), u8s.size()), 2, empty_bool_view()).is_ok);

    std::vector<int32_t> i32s = {-100, 200};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("i32"), MltColumnType_I32,
        u8_view_raw(i32s.data(), i32s.size() * sizeof(int32_t)), 2, empty_bool_view()).is_ok);

    std::vector<uint32_t> u32s = {100, 200};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("u32"), MltColumnType_U32,
        u8_view_raw(u32s.data(), u32s.size() * sizeof(uint32_t)), 2, empty_bool_view()).is_ok);

    std::vector<int64_t> i64s = {-1000, 2000};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("i64"), MltColumnType_I64,
        u8_view_raw(i64s.data(), i64s.size() * sizeof(int64_t)), 2, empty_bool_view()).is_ok);

    std::vector<uint64_t> u64s = {1000, 2000};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("u64"), MltColumnType_U64,
        u8_view_raw(u64s.data(), u64s.size() * sizeof(uint64_t)), 2, empty_bool_view()).is_ok);

    std::vector<float> f32s = {1.5f, 2.5f};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("f32"), MltColumnType_F32,
        u8_view_raw(f32s.data(), f32s.size() * sizeof(float)), 2, empty_bool_view()).is_ok);

    std::vector<double> f64s = {1.5, 2.5};
    ASSERT_TRUE(MltLayerEncoder_add_column(
        enc.get(), str_view("f64"), MltColumnType_F64,
        u8_view_raw(f64s.data(), f64s.size() * sizeof(double)), 2, empty_bool_view()).is_ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);
    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

// =========================================================================
// Error handling
// =========================================================================

TEST(FfiErrors, EncodeWithoutGeometryFails) {
    auto r = MltLayerEncoder_new(str_view("empty"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_FALSE(result.is_ok);

    UniqueError err(result.err);
    std::string msg = read_error(err.get());
    EXPECT_FALSE(msg.empty());
    EXPECT_NE(msg.find("geometry"), std::string::npos) << "error: " << msg;
}

TEST(FfiErrors, ConsumedEncoderReturnsError) {
    auto r = MltLayerEncoder_new(str_view("test"), 4096);
    ASSERT_TRUE(r.is_ok);
    auto* enc = r.ok; // raw pointer — we'll call encode twice

    std::vector<uint8_t> types = {0};
    std::vector<int32_t> verts = {1, 2};
    MltLayerEncoder_set_geometry(
        enc, u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());

    UniqueConfig config(MltEncoderConfig_new_default());

    // First encode succeeds
    auto r1 = MltLayerEncoder_encode(enc, config.get());
    ASSERT_TRUE(r1.is_ok);
    UniqueBuffer buf(r1.ok);

    // Second encode fails (consumed)
    auto r2 = MltLayerEncoder_encode(enc, config.get());
    ASSERT_FALSE(r2.is_ok);
    UniqueError err(r2.err);
    std::string msg = read_error(err.get());
    EXPECT_NE(msg.find("consumed"), std::string::npos) << "error: " << msg;

    // Clean up the encoder (it wasn't freed by encode since it was consumed on first call)
    MltLayerEncoder_destroy(enc);
}

// =========================================================================
// String columns
// =========================================================================

TEST(FfiStrings, EmptyStringColumn) {
    auto r = MltLayerEncoder_new(str_view("test"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    // Geometry: 2 points
    std::vector<uint8_t> types = {0, 0};
    std::vector<int32_t> verts = {1, 2, 3, 4};
    auto geo_r = MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
    ASSERT_TRUE(geo_r.is_ok);

    // String column with all nulls
    DiplomatStringView data_view = {"", 0};
    std::vector<uint32_t> offsets = {0, 0, 0};
    BoolVec present(2, false);
    auto str_r = MltLayerEncoder_add_string_column(
        enc.get(), str_view("name"), data_view, u32_view(offsets), present.view());
    ASSERT_TRUE(str_r.is_ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);
    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

TEST(FfiStrings, SingleElementLayer) {
    auto r = MltLayerEncoder_new(str_view("single"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    // 1 point
    std::vector<uint8_t> types = {0};
    std::vector<int32_t> verts = {42, 99};
    auto geo_r = MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
    ASSERT_TRUE(geo_r.is_ok);

    // 1 column value via unified add_column
    std::vector<int32_t> vals = {7};
    auto col_r = MltLayerEncoder_add_column(
        enc.get(), str_view("x"), MltColumnType_I32,
        u8_view_raw(vals.data(), vals.size() * sizeof(int32_t)),
        1, empty_bool_view());
    ASSERT_TRUE(col_r.is_ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);
    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

TEST(FfiStrings, BulkStrings) {
    auto r = MltLayerEncoder_new(str_view("test"), 4096);
    ASSERT_TRUE(r.is_ok);
    UniqueLayerEncoder enc(r.ok);

    // 3 points
    std::vector<uint8_t> types = {0, 0, 0};
    std::vector<int32_t> verts = {1, 2, 3, 4, 5, 6};
    auto geo_r = MltLayerEncoder_set_geometry(
        enc.get(), u8_view(types), i32_view(verts), empty_u32_view(), empty_u32_view(), empty_u32_view());
    ASSERT_TRUE(geo_r.is_ok);

    // 3 strings: "alpha", null, "gamma" — directly via add_string_column
    const char* data = "alphagamma";
    DiplomatStringView data_view = {data, 10};
    std::vector<uint32_t> offsets = {0, 5, 5, 10};
    BoolVec present(3, true);
    present.set(1, false);
    auto str_r = MltLayerEncoder_add_string_column(
        enc.get(), str_view("name"), data_view, u32_view(offsets), present.view());
    ASSERT_TRUE(str_r.is_ok);

    UniqueConfig config(MltEncoderConfig_new_default());
    auto result = MltLayerEncoder_encode(enc.release(), config.get());
    ASSERT_TRUE(result.is_ok);
    UniqueBuffer buf(result.ok);
    EXPECT_GT(MltEncodedBuffer_len(buf.get()), 0u);
}

// =========================================================================
// Config
// =========================================================================

TEST(FfiConfig, DefaultConfigAndSetters) {
    UniqueConfig config(MltEncoderConfig_new_default());
    ASSERT_NE(config, nullptr);

    // Exercise all setters — just verify they don't crash.
    MltEncoderConfig_set_tessellate(config.get(), true);
    MltEncoderConfig_set_try_morton_sort(config.get(), false);
    MltEncoderConfig_set_try_hilbert_sort(config.get(), false);
    MltEncoderConfig_set_try_id_sort(config.get(), false);
    MltEncoderConfig_set_allow_fsst(config.get(), false);
    MltEncoderConfig_set_allow_fast_pfor(config.get(), false);
    MltEncoderConfig_set_allow_shared_dict(config.get(), false);
}

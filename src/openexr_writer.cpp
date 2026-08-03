#include <OpenEXR/ImfChannelList.h>
#include <OpenEXR/ImfCompression.h>
#include <OpenEXR/ImfFrameBuffer.h>
#include <OpenEXR/ImfHeader.h>
#include <OpenEXR/ImfOutputFile.h>
#include <OpenEXR/ImfPixelType.h>

#include <memory>
#include <string>
#include <unordered_set>
#include <vector>

using namespace OPENEXR_IMF_NAMESPACE;

struct PlanetGenExrWriter {
    std::unique_ptr<OutputFile> file;
    int width;
    int height;
};

thread_local std::string planet_gen_exr_error;
static void set_error(const char* message) { planet_gen_exr_error = message; }

extern "C" const char* planet_gen_exr_last_error() {
    return planet_gen_exr_error.c_str();
}

extern "C" PlanetGenExrWriter* planet_gen_exr_open(
    const char* path, int width, int height, const char* const* channels, int channel_count
) {
    if (path == nullptr || width <= 0 || height <= 0 || channels == nullptr || channel_count <= 0) {
        set_error("invalid OpenEXR output path, dimensions, or channels");
        return nullptr;
    }
    try {
        Header header(width, height);
        header.compression() = ZIP_COMPRESSION;
        std::unordered_set<std::string> names;
        for (int index = 0; index < channel_count; ++index) {
            if (channels[index] == nullptr || channels[index][0] == '\0' || !names.insert(channels[index]).second) {
                set_error("OpenEXR channels must be non-empty and unique");
                return nullptr;
            }
            header.channels().insert(channels[index], Channel(FLOAT));
        }
        return new PlanetGenExrWriter { std::make_unique<OutputFile>(path, header), width, height };
    } catch (const std::exception& error) {
        planet_gen_exr_error = error.what();
        return nullptr;
    }
}

extern "C" int planet_gen_exr_write_scanline(
    PlanetGenExrWriter* writer, int y, const float* pixels, const char* const* channels, int channel_count
) {
    if (writer == nullptr || pixels == nullptr || channels == nullptr || channel_count <= 0
        || y != writer->file->currentScanLine() || y < 0 || y >= writer->height) {
        set_error("invalid OpenEXR scanline write");
        return 0;
    }
    try {
        FrameBuffer frame_buffer;
        const size_t pixel_stride = sizeof(float) * channel_count;
        for (int channel = 0; channel < channel_count; ++channel) {
            if (channels[channel] == nullptr) {
                set_error("invalid OpenEXR channel name");
                return 0;
            }
            char* base = reinterpret_cast<char*>(const_cast<float*>(pixels)) + sizeof(float) * channel;
            frame_buffer.insert(channels[channel], Slice(FLOAT, base, pixel_stride, 0));
        }
        writer->file->setFrameBuffer(frame_buffer);
        writer->file->writePixels(1);
        return 1;
    } catch (const std::exception& error) {
        planet_gen_exr_error = error.what();
        return 0;
    }
}

extern "C" void planet_gen_exr_close(PlanetGenExrWriter* writer) {
    delete writer;
}

#include <OpenEXR/ImfChannelList.h>
#include <OpenEXR/ImfCompression.h>
#include <OpenEXR/ImfFrameBuffer.h>
#include <OpenEXR/ImfHeader.h>
#include <OpenEXR/ImfOutputFile.h>
#include <OpenEXR/ImfPixelType.h>

#include <memory>
#include <string>

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

extern "C" PlanetGenExrWriter* planet_gen_exr_open(const char* path, int width, int height) {
    if (path == nullptr || width <= 0 || height <= 0) { set_error("invalid OpenEXR output path or dimensions"); return nullptr; }
    try {
        Header header(width, height);
        header.compression() = ZIP_COMPRESSION;
        for (const char* channel : {"R", "G", "B", "A"}) {
            header.channels().insert(channel, Channel(FLOAT));
        }
        return new PlanetGenExrWriter { std::make_unique<OutputFile>(path, header), width, height };
    } catch (const std::exception& error) {
        planet_gen_exr_error = error.what();
        return nullptr;
    }
}

extern "C" int planet_gen_exr_write_rgba_scanline(PlanetGenExrWriter* writer, int y, const float* rgba) {
    if (writer == nullptr || rgba == nullptr || y != writer->file->currentScanLine() || y < 0 || y >= writer->height) {
        set_error("invalid OpenEXR scanline write");
        return 0;
    }
    try {
        FrameBuffer frame_buffer;
        const size_t pixel_stride = sizeof(float) * 4;
        for (int channel = 0; channel < 4; ++channel) {
            const char* name = channel == 0 ? "R" : channel == 1 ? "G" : channel == 2 ? "B" : "A";
            char* base = reinterpret_cast<char*>(const_cast<float*>(rgba)) + sizeof(float) * channel;
            frame_buffer.insert(name, Slice(FLOAT, base, pixel_stride, 0));
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

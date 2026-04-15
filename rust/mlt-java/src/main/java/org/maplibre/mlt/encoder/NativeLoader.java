package org.maplibre.mlt.encoder;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.List;
import java.util.Locale;

/**
 * Loads the {@code mlt_ffi} native library from the classpath (classifier JAR) or falls back to
 * {@link System#loadLibrary} for local development with {@code -Djava.library.path}.
 *
 * <p>Classpath convention: {@code /native/<os>-<arch>/<libname>} where {@code os} is one of
 * {@code linux}, {@code macos}, {@code windows} and {@code arch} is one of {@code x86_64},
 * {@code aarch64}.
 */
final class NativeLoader {

  private static final String LIB_NAME = "mlt_ffi";
  private static volatile boolean loaded;

  private NativeLoader() {}

  /** Load the native library. Safe to call multiple times — only the first call has effect. */
  static void load() {
    if (loaded) {
      return;
    }
    synchronized (NativeLoader.class) {
      if (loaded) {
        return;
      }
      doLoad();
      loaded = true;
    }
  }

  private static void doLoad() {
    String os = detectOs();
    String arch = detectArch();
    String libFileName = System.mapLibraryName(LIB_NAME);

    // Build candidate resource paths: prefer libc-specific variant on Linux (musl vs glibc)
    List<String> candidates;
    if ("linux".equals(os) && isMusl(arch)) {
      candidates =
          List.of(
              "/native/linux-" + arch + "-musl/" + libFileName,
              "/native/linux-" + arch + "/" + libFileName);
    } else {
      candidates = List.of("/native/" + os + "-" + arch + "/" + libFileName);
    }

    for (String resourcePath : candidates) {
      InputStream in = NativeLoader.class.getResourceAsStream(resourcePath);
      if (in != null) {
        loadFromClasspath(in, libFileName);
        return;
      }
    }

    // Fall back to java.library.path (local dev with -Djava.library.path=build/natives)
    System.loadLibrary(LIB_NAME);
  }

  private static void loadFromClasspath(InputStream in, String libFileName) {
    try (in) {
      String suffix = libFileName.substring(libFileName.lastIndexOf('.'));
      Path tmp = Files.createTempFile("mlt_ffi-", suffix);
      tmp.toFile().deleteOnExit();
      Files.copy(in, tmp, StandardCopyOption.REPLACE_EXISTING);
      System.load(tmp.toAbsolutePath().toString());
    } catch (IOException e) {
      throw new UnsatisfiedLinkError("Failed to extract native library: " + e.getMessage());
    }
  }

  private static String detectOs() {
    String name = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
    if (name.contains("linux")) {
      return "linux";
    }
    if (name.contains("mac") || name.contains("darwin")) {
      return "macos";
    }
    if (name.contains("windows")) {
      return "windows";
    }
    throw new UnsatisfiedLinkError("Unsupported OS: " + name);
  }

  private static String detectArch() {
    String arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);
    return switch (arch) {
      case "amd64", "x86_64" -> "x86_64";
      case "aarch64", "arm64" -> "aarch64";
      default -> throw new UnsatisfiedLinkError("Unsupported architecture: " + arch);
    };
  }

  /** Detect musl libc by checking for the musl dynamic linker. */
  private static boolean isMusl(String arch) {
    return Files.exists(Path.of("/lib/ld-musl-" + arch + ".so.1"));
  }
}

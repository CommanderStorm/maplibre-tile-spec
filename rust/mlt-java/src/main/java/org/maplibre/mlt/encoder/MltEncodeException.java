package org.maplibre.mlt.encoder;

import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

/** Structured exception thrown when the native MLT encoder fails. */
@SuppressWarnings("serial")
public class MltEncodeException extends RuntimeException {

  /** The encoding phase in which the error occurred. */
  public enum Phase {
    INIT,
    LAYER_CREATE,
    SET_IDS,
    SET_GEOMETRY,
    ADD_COLUMN,
    TILE_CREATE,
    TILE_ADD_LAYER,
    TILE_ENCODE,
    BUFFER_READ,
  }

  private final @NotNull Phase phase;
  private final @Nullable String layerName;
  private final @Nullable String columnName;

  public MltEncodeException(
      @NotNull Phase phase,
      @Nullable String layerName,
      @Nullable String columnName,
      @NotNull String nativeError) {
    super(buildMessage(phase, layerName, columnName, nativeError));
    this.phase = phase;
    this.layerName = layerName;
    this.columnName = columnName;
  }

  public @NotNull Phase phase() {
    return phase;
  }

  public @Nullable String layerName() {
    return layerName;
  }

  public @Nullable String columnName() {
    return columnName;
  }

  private static String buildMessage(
      Phase phase, @Nullable String layerName, @Nullable String columnName, String nativeError) {
    var sb = new StringBuilder();
    sb.append(phase);
    if (layerName != null) {
      sb.append(" [layer=").append(layerName);
      if (columnName != null) {
        sb.append(", column=").append(columnName);
      }
      sb.append(']');
    }
    sb.append(": ").append(nativeError);
    return sb.toString();
  }
}

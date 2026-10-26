package xz.runtime;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;

/**
 * Batch vertex emitter used by the native avatar runtime.
 *
 * Rust packs baked, posed geometry into a flat float[] and calls into here once
 * per texture bucket, so a model costs one JNI transition instead of one per
 * vertex.
 *
 * Both emitters MUST feed the incoming {@link PoseStack.Pose} to the
 * VertexConsumer. submitCustomGeometry defers the draw: the Pose handed to the
 * callback is the only thing that carries the avatar's model-to-camera matrix
 * (root frame, limb transforms, entity position). Emitting bare coordinates
 * publishes model-space verts straight into camera space, which parks the whole
 * avatar on the camera origin -- it then reads as a gigantic model filling the
 * screen. Use addVertex(pose, ...) / setNormal(pose, ...), never the
 * pose-less overloads.
 *
 * The sibling MeshEmit.class is what actually ships (include_bytes! in
 * src/platform/jvmti/trampoline.rs) -- editing this file has no effect until you
 * run scripts/build_mesh_emit_class.py to recompile it.
 */
public final class MeshEmit {
    /**
     * Packed layout, 9 floats per vertex:
     * x, y, z, u, v, nx, ny, nz, colorBits (ARGB int bit-cast to float).
     */
    public static void emitPacked(
            PoseStack.Pose pose,
            VertexConsumer buffer,
            float[] data,
            int vertexCount,
            int overlayCoords,
            int lightCoords) {
        for (int i = 0, o = 0; i < vertexCount; i++, o += 9) {
            buffer.addVertex(pose, data[o], data[o + 1], data[o + 2])
                    .setColor(Float.floatToRawIntBits(data[o + 8]))
                    .setUv(data[o + 3], data[o + 4])
                    .setOverlay(overlayCoords)
                    .setLight(lightCoords)
                    .setNormal(pose, data[o + 5], data[o + 6], data[o + 7]);
        }
    }

    /**
     * Packed layout, 8 floats per vertex:
     * x, y, z, colorBits (ARGB int bit-cast to float), lineWidth, dx, dy, dz.
     *
     * The last three are the line's DIRECTION, and they matter: the LINES pipeline
     * (DefaultVertexFormat.POSITION_COLOR_NORMAL_LINE_WIDTH) expands each segment
     * into a screen-space quad using the normal as that direction. A hardcoded
     * (0, 1, 0) — which this method used to send — collapses any segment that is
     * not vertical into a zero-area quad, so the line is submitted but nothing is
     * ever drawn. Feed the real per-segment direction.
     *
     * Direction must go through the Pose like the position does, otherwise it is
     * interpreted in camera space and the expansion is wrong as soon as the camera
     * turns.
     */
    public static void emitLines(
            PoseStack.Pose pose,
            VertexConsumer buffer,
            float[] data,
            int vertexCount) {
        for (int i = 0, o = 0; i < vertexCount; i++, o += 8) {
            buffer.addVertex(pose, data[o], data[o + 1], data[o + 2])
                    .setColor(Float.floatToRawIntBits(data[o + 3]))
                    .setLineWidth(data[o + 4])
                    .setNormal(pose, data[o + 5], data[o + 6], data[o + 7]);
        }
    }

    private MeshEmit() {
    }
}

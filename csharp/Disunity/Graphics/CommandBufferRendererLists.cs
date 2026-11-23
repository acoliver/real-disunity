namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    /// <summary>Records a draw of the renderer list into the targets bound at this point of the buffer. A list that is not valid
    /// throws <see cref="ArgumentException"/>. The list belongs to the frame that made it: the world refuses the draw when the
    /// buffer is executed after that frame ended. The batch draws the list accepts follow it (see <see cref="BatchListDraws"/>).</summary>
    public void DrawRendererList(RendererList rendererList)
    {
        ThrowIfReleased();
        if (!rendererList.isValid)
            throw new ArgumentException("CommandBuffer.DrawRendererList: the renderer list is not valid", nameof(rendererList));
        if (rendererList.UiOverlay) return;
        var (id, batches) = (rendererList.Id, rendererList.Batches);
        Record(() =>
        {
            WorldGraphics.RecordDrawList(id);
            batches?.Record();
        });
    }
}

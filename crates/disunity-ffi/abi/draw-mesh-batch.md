# Mesh draw of a batch of instances

A command buffer records a mesh draw of a batch of instances (disunity_offscreen_draw_mesh_batch, kind 23): a stored mesh, a material pass and a window of a compute buffer that the shader reads per instance, the metadata words of the properties the game set by name and the indexes of the instances to draw; the graphics snapshot carries it from wire version 20, with the buffer it names.

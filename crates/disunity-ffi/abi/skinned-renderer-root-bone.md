# Skinned renderer root bone

disunity_skinned_renderer_write names the mesh identity a skinned mesh renderer draws and its root bone with the bone's bind pose (16 column-major floats), which place the mesh when its bones are at rest; a renderer with no root bone has no placement and is left out of frames, and a mesh renderer or filter has no skin.

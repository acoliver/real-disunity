# Canvas renderer world material

A canvas renderer keeps the world material of its main slot 0 (the world_material argument of disunity_canvas_renderer_material_write, 0 for none), and DisunityUiDraw names it with material and material_generation (the draw record grows from 160 to 176 bytes), 0 for none.

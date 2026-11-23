# Stored texture dimension

disunity_texture_store refuses a texture record whose dimension is 0: a 2D texture states dimension 2, and the host reads only 2, 3, 4 and 5.

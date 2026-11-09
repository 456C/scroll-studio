package dev.lightbound.client.mixin;

import net.minecraft.client.renderer.chunk.SectionCopy;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.PalettedContainer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(SectionCopy.class)
public interface SectionCopyAccessor {
    @Accessor("section") PalettedContainer<BlockState> lightbound$states();
}

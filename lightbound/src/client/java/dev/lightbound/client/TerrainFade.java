package dev.lightbound.client;

import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;
import net.minecraft.core.BlockPos;
import net.minecraft.client.Minecraft;
import net.minecraft.util.LightCoordsUtil;

/** Six-tick visual transitions, animated on the GPU between two bounded mesh updates. */
public final class TerrainFade {
    private static final int DURATION = 6;
    private static final int LIMIT = 65536;
    private static final ConcurrentHashMap<Long, State> states = new ConcurrentHashMap<>();
    private static final ConcurrentLinkedQueue<Pending> pending = new ConcurrentLinkedQueue<>();
    private static volatile long tick;
    private record State(boolean lit, double start, long seen, boolean lichenOnly) {}
    private record Pending(long pos, long due) {}
    private TerrainFade() {}

    public static void clear() { states.clear(); pending.clear(); }
    public static void tick(Minecraft client) {
        if (client.level == null) { clear(); return; }
        tick = client.level.getGameTime();
        for(int n=0;n<8192;n++) {
            Pending next=pending.peek();
            if(next==null || next.due>tick) break;
            pending.poll();
            BlockPos p=BlockPos.of(next.pos);
            BuiltinDynamicLighting.markFadeSection(p);
        }
        if(tick%200==0) states.entrySet().removeIf(e->tick-e.getValue().seen>200);
    }
    public static int light(BlockPos pos, boolean lit, int normal, boolean lichenOnly) {
        long now=tick, key=pos.asLong();
        if(states.size()>=LIMIT && !states.containsKey(key)) return normal;
        State state=states.compute(key,(k,old)-> {
            if(old==null) return new State(lit, now-DURATION, now, lichenOnly);
            boolean localFade = lit ? lichenOnly : old.lichenOnly;
            if(old.lit==lit) return new State(lit,old.start,now,localFade);
            double age=Math.clamp((now-old.start)/DURATION,0,1);
            double brightness=old.lit?age:1-age;
            double start=now-(lit?brightness:1-brightness)*DURATION;
            pending.add(new Pending(key,now+DURATION+1));
            return new State(lit,start,now,localFade);
        });
        if(now-state.start>=DURATION) return normal;
        // The lower pair also applies the lichen distance fade throughout this transition.
        int marker = (state.lichenOnly ? 252 : 254) + (lit ? 1 : 0);
        return LightCoordsUtil.smoothPack(Math.floorMod((int)Math.floor(state.start),240), marker);
    }
}

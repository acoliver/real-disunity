local function leaf(a)
  local got = coroutine.yield('leaf', a)
  return got + 1
end
function nested(x)
  local r = leaf(x)
  return r * 2
end
function numfor(n)
  local sum = 0
  for i = 1, n do
    local got = coroutine.yield('for', i)
    sum = sum + got
  end
  return sum
end
function listloop(t)
  local out = {}
  for i, v in ipairs(t) do
    local got = coroutine.yield('item', i, v)
    out[#out + 1] = got
  end
  return #out, out[1], out[2], out[3]
end
function va(a, ...)
  local n = select('#', ...)
  local got = coroutine.yield('va', a, n)
  return got, ...
end
function tailer(x)
  return coroutine.yield('tail', x)
end
function protected(x)
  local ok, v = pcall(leaf, x)
  return ok, v
end
function upv(n)
  local count = n
  local function bump()
    count = count + 1
    return count
  end
  local got = coroutine.yield('upv', count)
  local a = bump()
  return a, got
end
function temps(t)
  local r = t:get(coroutine.yield('x'))
  return r
end
function share(n)
  local count = n
  local function raise()
    count = count + 1
  end
  local got = coroutine.yield('share', count)
  raise()
  local seen = count
  count = count + 10
  raise()
  return seen, count, got
end
function ticker(limit, i)
  local got = coroutine.yield('tick', i)
  if i < limit then
    return i + 1
  end
end
function ticks(limit)
  local sum = 0
  for i in ticker, limit, 0 do
    sum = sum + i
  end
  return sum
end
local seed = {}
local count = 1
function stash(k)
  seed[coroutine.yield('stash', k)] = count
  count = count + 1
  return count, seed[k]
end
function pick(x)
  return x
end
function guarded(n)
  local total = 0
  while total < n do
    local ok, v = pcall(leaf, pick(total))
    total = total + v
  end
  return total
end
function halves(n)
  local half = n / 2
  local got = coroutine.yield('half', half)
  local more = coroutine.yield('more', got / 4 + half)
  return more / 2, got / 2
end

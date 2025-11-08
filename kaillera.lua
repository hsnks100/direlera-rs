-- Kaillera Protocol Dissector for Wireshark
-- This plugin parses Kaillera game networking protocol packets

local proto_kaillera = Proto.new("kaillera", "Kaillera Protocol")

-- Value string tables
local vs_conn_type = {
    [6] = "Bad",
    [5] = "Low",
    [4] = "Average",
    [3] = "Good",
    [2] = "Excellent",
    [1] = "LAN",
}
local vs_game_status = {
    [0] = "Waiting",
    [1] = "Playing",
    [2] = "Netsync",
}
local vs_player_status = {
    [0] = "Playing",
    [1] = "Idle",
}

-- Protocol field definitions (common)
local f_messages         = ProtoField.uint8("kaillera.messages", "Messages", base.DEC)
local f_msg_seq          = ProtoField.uint16("kaillera.msg_seq", "Message Sequence", base.DEC, nil, 0, "Sequence number (LE)")
local f_message_length   = ProtoField.uint16("kaillera.message_length", "Message Length", base.DEC, nil, 0, "Length incl. type (LE)")
local f_message_type     = ProtoField.uint8("kaillera.message_type", "Message Type", base.HEX)

-- Generic string/data helpers
local f_nb               = ProtoField.string("kaillera.nb", "NB (StringZ)")
local f_data_len16       = ProtoField.uint16("kaillera.data_len", "Data Length", base.DEC)
local f_data_bytes       = ProtoField.bytes("kaillera.data", "Data")

-- User-related
local f_username         = ProtoField.string("kaillera.username", "Username")
local f_user_id          = ProtoField.uint16("kaillera.user_id", "User ID", base.DEC)
local f_ping             = ProtoField.uint32("kaillera.ping", "Ping", base.DEC)
local f_conn_type        = ProtoField.uint8("kaillera.conn_type", "Connection Type", base.DEC, vs_conn_type)

-- Server status lists
local f_num_users        = ProtoField.uint32("kaillera.num_users", "Users (server)", base.DEC)
local f_num_games        = ProtoField.uint32("kaillera.num_games", "Games (server)", base.DEC)
local f_player_status    = ProtoField.uint8("kaillera.player_status", "Player Status", base.DEC, vs_player_status)

-- Game-related
local f_game_name        = ProtoField.string("kaillera.game_name", "Game Name")
local f_game_id          = ProtoField.uint32("kaillera.game_id", "Game ID", base.DEC)
local f_emulator_name    = ProtoField.string("kaillera.emulator", "Emulator")
local f_room_owner       = ProtoField.string("kaillera.room_owner", "Room Owner")
local f_players_str      = ProtoField.string("kaillera.players_str", "Players (cur/max)")
local f_game_status      = ProtoField.uint8("kaillera.game_status", "Game Status", base.DEC, vs_game_status)

-- Start game
local f_frame_delay      = ProtoField.uint16("kaillera.frame_delay", "Frame Delay", base.DEC)
local f_your_player_num  = ProtoField.uint8("kaillera.your_player_num", "Your Player Number", base.DEC)
local f_total_players    = ProtoField.uint8("kaillera.total_players", "Total Players", base.DEC)
local f_max_players      = ProtoField.uint8("kaillera.max_players", "Max Players", base.DEC)

-- Game data/cache
local f_cache_pos        = ProtoField.uint8("kaillera.cache_pos", "Cache Position", base.DEC)

proto_kaillera.fields = {
    f_messages, f_msg_seq, f_message_length, f_message_type,
    f_nb, f_data_len16, f_data_bytes,
    f_username, f_user_id, f_ping, f_conn_type,
    f_num_users, f_num_games, f_player_status,
    f_game_name, f_game_id, f_emulator_name, f_room_owner, f_players_str, f_game_status,
    f_frame_delay, f_your_player_num, f_total_players, f_max_players,
    f_cache_pos,
}

local function get_message_type_description(msg_type)
    local type_descriptions = {
        [0x01] = "User Quit",
        [0x02] = "User Joined",
        [0x03] = "User Login Information",
        [0x04] = "Server Status",
        [0x05] = "Server to Client ACK",
        [0x06] = "Client to Server ACK",
        [0x07] = "Global Chat",
        [0x08] = "Game Chat",
        [0x09] = "Client Keep Alive",
        [0x0A] = "Create Game",
        [0x0B] = "Quit Game",
        [0x0C] = "Join Game",
        [0x0D] = "Player Information",
        [0x0E] = "Update Game Status",
        [0x0F] = "Kick User from Game",
        [0x10] = "Close Game",
        [0x11] = "Start Game",
        [0x12] = "Game Data",
        [0x13] = "Game Cache",
        [0x14] = "Drop Game",
        [0x15] = "Ready to Play",
        [0x16] = "Connection Rejected",
        [0x17] = "Server Information Message",
    }
    return type_descriptions[msg_type] or string.format("Unknown (0x%02x)", msg_type)
end

-- Validate message type
local function is_valid_message_type(msg_type)
    local valid_types = {
        [0x01] = true, [0x02] = true, [0x03] = true, [0x04] = true, [0x05] = true,
        [0x06] = true, [0x07] = true, [0x08] = true, [0x09] = true, [0x0A] = true,
        [0x0B] = true, [0x0C] = true, [0x0D] = true, [0x0E] = true, [0x0F] = true,
        [0x10] = true, [0x11] = true, [0x12] = true, [0x13] = true, [0x14] = true,
        [0x15] = true, [0x16] = true, [0x17] = true
    }
    return valid_types[msg_type] or false
end

-- Short hex preview of bytes
local function fmt_bytes_hex(tvb, maxn)
    local n = math.min(tvb:len(), maxn or 16)
    if n <= 0 then return "" end
    local t = {}
    for i = 0, n - 1 do
        t[#t+1] = string.format("%02X", tvb(i,1):uint())
    end
    local suffix = (tvb:len() > n) and "…" or ""
    return table.concat(t, " ") .. suffix
end

-- Read a zero-terminated string; returns str, consumed_len
local function read_nb(tvb, offset)
    local s = tvb(offset):stringz()
    return s, #s + 1
end

-- Per-type payload dissector (tvb covers ONLY the payload after type byte)
-- Returns optional summary string for info column
local function dissect_payload_by_type(msg_type, tvb, subtree)
    local offset = 0
    local info_summary = nil

    local function safe_add(field, len, is_le, label)
        if offset + len <= tvb:len() then
            local r = tvb(offset, len)
            if label then
                if is_le then subtree:add_le(field, r, label) else subtree:add(field, r, label) end
            else
                if is_le then subtree:add_le(field, r) else subtree:add(field, r) end
            end
            offset = offset + len
            return r
        end
    end

    local function add_nb(field)
        if offset < tvb:len() then
            local s, n = read_nb(tvb, offset)
            subtree:add(field, tvb(offset, n), s)
            offset = offset + n
            return s, n
        end
    end

    if msg_type == 0x01 then
        -- User Quit
        add_nb(f_nb)                            -- NB (Client)/Username (Server)
        safe_add(f_user_id, 2, true)            -- 2B
        add_nb(f_nb)                            -- Message
    elseif msg_type == 0x02 then
        -- User joined (Server)
        add_nb(f_username)
        safe_add(f_user_id, 2, true)
        safe_add(f_ping, 4, true)
        safe_add(f_conn_type, 1, false)
    elseif msg_type == 0x03 then
        -- User Login Information (Client)
        add_nb(f_username)
        add_nb(f_emulator_name)
        safe_add(f_conn_type, 1, false)
    elseif msg_type == 0x04 then
        -- Server Status
        add_nb(f_nb)                            -- Empty
        safe_add(f_num_users, 4, true)
        safe_add(f_num_games, 4, true)
        if offset < tvb:len() then
            subtree:add(f_data_bytes, tvb(offset), "Users/Games Lists")
        end
    elseif msg_type == 0x05 or msg_type == 0x06 then
        -- ACKs
        add_nb(f_nb)                            -- Empty
        for _ = 1, 4 do
            safe_add(f_ping, 4, true)           -- Spec lists 00,01,02,03 u32s
        end
    elseif msg_type == 0x07 or msg_type == 0x08 then
        -- Global/Game Chat
        add_nb(f_nb)                            -- Empty or Username (server)
        add_nb(f_nb)                            -- Message
    elseif msg_type == 0x09 then
        -- Client Keep Alive
        add_nb(f_nb)                            -- Empty
    elseif msg_type == 0x0A then
        -- Create Game
        add_nb(f_nb)
        add_nb(f_game_name)
        if offset < tvb:len() then
            local emu_try, n2 = read_nb(tvb, offset)
            if offset + n2 + 4 <= tvb:len() then
                subtree:add(f_emulator_name, tvb(offset, n2), emu_try)
                offset = offset + n2
                safe_add(f_game_id, 4, true)
            else
                add_nb(f_nb)
                if offset < tvb:len() then
                    subtree:add(f_data_bytes, tvb(offset))
                end
            end
        end
    elseif msg_type == 0x0B then
        -- Quit Game
        add_nb(f_nb)
        safe_add(f_user_id, 2, true)
    elseif msg_type == 0x0C then
        -- Join Game
        add_nb(f_nb)
        safe_add(f_game_id, 4, true)
        add_nb(f_nb)
        safe_add(f_ping, 4, true)               -- 0x00 per spec for client req
        safe_add(f_user_id, 2, true)            -- 0xFF per client req
        safe_add(f_conn_type, 1, false)
    elseif msg_type == 0x0D then
        -- Player Information (Server)
        add_nb(f_nb)
        safe_add(f_num_users, 4, true)          -- users in room (not including you)
        add_nb(f_username)
        safe_add(f_ping, 4, true)
        safe_add(f_user_id, 2, true)
        safe_add(f_conn_type, 1, false)
    elseif msg_type == 0x0E then
        -- Update Game Status
        add_nb(f_nb)
        safe_add(f_game_id, 4, true)
        safe_add(f_game_status, 1, false)
        safe_add(f_total_players, 1, false)
        safe_add(f_max_players, 1, false)
    elseif msg_type == 0x0F then
        -- Kick User from Game
        add_nb(f_nb)
        safe_add(f_user_id, 2, true)
    elseif msg_type == 0x10 then
        -- Close Game
        add_nb(f_nb)
        safe_add(f_game_id, 4, true)
    elseif msg_type == 0x11 then
        -- Start Game
        add_nb(f_nb)                            -- client: then 2B FF,1B FF,1B FF
        if offset + 4 <= tvb:len() then         -- server: frame delay, your player, total players
            subtree:add_le(f_frame_delay, tvb(offset, 2))
            offset = offset + 2
            safe_add(f_your_player_num, 1, false)
            safe_add(f_total_players, 1, false)
        end
    elseif msg_type == 0x12 then
        -- Game Data
        add_nb(f_nb)
        local len_f = safe_add(f_data_len16, 2, true)
        local data_len = len_f and len_f:le_uint() or 0
        if data_len > 0 and offset + data_len <= tvb:len() then
            local data_tvb = tvb(offset, data_len)
            subtree:add(f_data_bytes, data_tvb)
            info_summary = string.format("game_data %dB [%s]", data_len, fmt_bytes_hex(data_tvb, 12))
            offset = offset + data_len
        else
            info_summary = "game_data 0B"
        end
    elseif msg_type == 0x13 then
        -- Game Cache
        add_nb(f_nb)
        local cp = safe_add(f_cache_pos, 1, false)
        local cache_pos = cp and cp:uint() or 0
        if offset < tvb:len() then
            local rest = tvb(offset)
            subtree:add(f_data_bytes, rest, "Cache/Data Remainder")
        end
        info_summary = string.format("game_cache pos=%d", cache_pos)
    elseif msg_type == 0x14 then
        -- Drop Game
        add_nb(f_nb)
        safe_add(f_your_player_num, 1, false)   -- "which player number dropped"
    elseif msg_type == 0x15 then
        -- Ready to Play Signal
        add_nb(f_nb)
    elseif msg_type == 0x16 then
        -- Connection Rejected
        add_nb(f_username)
        safe_add(f_user_id, 2, true)
        add_nb(f_nb)                            -- Message
    elseif msg_type == 0x17 then
        -- Server Information Message
        add_nb(f_nb)                            -- "Server\0"
        add_nb(f_nb)                            -- Message
    else
        if tvb:len() > 0 then
            subtree:add(f_data_bytes, tvb)
        end
    end

    return info_summary
end

-- Analyze packet for redundancy and recovery patterns
local function analyze_packet_redundancy(messages_data)
    if #messages_data < 2 then return "" end
    local info_parts = {}
    local latest_seq = messages_data[1].seq_num
    for i = 2, #messages_data do
        local expected_seq = latest_seq - (i - 1)
        local actual_seq = messages_data[i].seq_num
        if actual_seq ~= expected_seq then
            table.insert(info_parts, string.format("GAP: M%d seq=%d (expected %d)", i, actual_seq, expected_seq))
        end
    end
    return table.concat(info_parts, ", ")
end

-- Main dissector function
function proto_kaillera.dissector(buffer, pinfo, tree)
    pinfo.cols.protocol = "KAILLERA"
    local blen = buffer:len()
    
    -- Since we already validated in heuristic checker, we can be more confident
    -- but still do basic checks
    if blen < 1 then return end

    local root = tree:add(proto_kaillera, buffer(0))

    -- Get number of messages in this packet
    local message_count = buffer(0, 1):uint()
    root:add(f_messages, buffer(0, 1))

    local buffer_index = 1
    local messages_data = {}
    local latest_message_info = ""

    -- Parse each message in the packet
    for i = 0, message_count - 1 do
        if buffer_index + 5 > blen then break end
        local msg_start_index = buffer_index

        -- Message sequence number (LE)
        local seq_num = buffer(buffer_index, 2):le_uint()
        buffer_index = buffer_index + 2

        -- Message length (LE)
        local msg_length = buffer(buffer_index, 2):le_uint()
        buffer_index = buffer_index + 2

        -- Message type
        if buffer_index + 1 > blen then break end
        local msg_type = buffer(buffer_index, 1):uint()
        buffer_index = buffer_index + 1

        -- Compute total span of this message including header
        local total_len = 4 + msg_length -- 2 (seq) + 2 (len) + msg_length (incl. type+data)
        local span_len = math.min(total_len, blen - msg_start_index)

        -- Store metadata
        local msg_data = {
            index = i + 1,
            seq_num = seq_num,
            msg_type = msg_type,
            msg_length = msg_length,
            start_index = msg_start_index,
            total_length = total_len,
        }
        table.insert(messages_data, msg_data)

        -- Message subtree
        local msg_label = string.format("Message %d (Seq: %d) - %s",
            i + 1, seq_num, i == 0 and "LATEST" or string.format("History %d", i))
        local msg_range = buffer(msg_start_index, span_len)
        local msg_subtree = root:add(proto_kaillera, msg_range, msg_label)

        -- Header fields
        msg_subtree:add_le(f_msg_seq, buffer(msg_start_index, 2))
        msg_subtree:add_le(f_message_length, buffer(msg_start_index + 2, 2))
        msg_subtree:add(f_message_type, buffer(msg_start_index + 4, 1))

        -- Type description
        local type_desc = get_message_type_description(msg_type)
        msg_subtree:add(buffer(msg_start_index + 4, 1), string.format("Type Description: %s", type_desc))

        -- Payload
        local payload_info = nil
        local remaining = msg_length - 1 -- type byte already consumed
        if remaining > 0 and buffer_index + remaining <= blen then
            local payload_range = buffer(buffer_index, remaining)
            local payload_tree = msg_subtree:add(payload_range, string.format("Payload (%d bytes)", remaining))
            payload_info = dissect_payload_by_type(msg_type, payload_range, payload_tree)
        end

        -- Latest message info: include compact bytes for game data/cache
        if i == 0 then
            local base_info = string.format("%s (seq:%d)", type_desc, seq_num)
            if payload_info then
                latest_message_info = string.format("%s | %s", base_info, payload_info)
            else
                latest_message_info = base_info
            end
        end

        -- Advance to next message
        buffer_index = buffer_index + math.max(0, remaining)
    end

    -- Analyze packet for redundancy and recovery info
    local redundancy_info = analyze_packet_redundancy(messages_data)

    -- Set packet info display (focused on latest message)
    local info = latest_message_info
    if redundancy_info ~= "" then
        info = info .. " | " .. redundancy_info
    end

    pinfo.cols.info = string.format("%s [%s->%s]", info, tostring(pinfo.src_port), tostring(pinfo.dst_port))
end

-- Validate if this looks like a valid Kaillera packet
local function is_valid_kaillera_packet(buffer)
    local blen = buffer:len()
    
    -- Basic packet size check
    if blen < 1 then return false end
    
    -- Get message count
    local message_count = buffer(0, 1):uint()
    
    -- Validate message count
    if message_count == 0 or message_count > 10 then return false end
    
    -- Check if we have enough data for at least one message header
    if blen < 6 then return false end  -- 1 (count) + 5 (min header: seq+len+type)
    
    local buffer_index = 1
    
    -- Validate first message to determine if this is Kaillera
    for i = 0, math.min(message_count - 1, 2) do  -- Check up to 3 messages
        if buffer_index + 5 > blen then return false end
        
        -- Message length
        local msg_length = buffer(buffer_index + 2, 2):le_uint()
        
        -- Validate message length (reasonable limits)
        if msg_length < 1 or msg_length > 65535 then return false end
        
        -- Message type
        local msg_type = buffer(buffer_index + 4, 1):uint()
        
        -- Check if message type is valid Kaillera type
        if not is_valid_message_type(msg_type) then return false end
        
        -- Check if message fits in packet
        if buffer_index + 4 + msg_length > blen then return false end
        
        -- Move to next message
        buffer_index = buffer_index + 4 + msg_length
    end
    
    return true
end

-- Heuristic checker function
local function heuristic_checker(buffer, pinfo, tree)
    -- Only process if this looks like a valid Kaillera packet
    if is_valid_kaillera_packet(buffer) then
        proto_kaillera.dissector(buffer, pinfo, tree)
        return true
    end
    return false
end

-- Register the protocol with UDP
proto_kaillera:register_heuristic("udp", heuristic_checker)
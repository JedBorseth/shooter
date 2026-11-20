import { z } from 'zod';
function formatError(error) {
    return {
        isError: true,
        content: [{ type: 'text', text: error instanceof Error ? error.message : JSON.stringify(error) }]
    };
}
export function registerViewportTools(server, bridge) {
    server.tool('godot_get_viewport_screenshot', 'Capture a screenshot of a specific Godot Editor viewport.', {
        viewport: z.enum(['2d', '3d']).optional().describe('Which viewport to capture. Defaults to 3d.')
    }, async ({ viewport = '3d' }) => {
        try {
            const res = await bridge.sendRequest('get_viewport_screenshot', { viewport });
            return {
                content: [{
                        type: 'image',
                        data: res.image_base64,
                        mimeType: 'image/png'
                    }]
            };
        }
        catch (e) {
            return formatError(e);
        }
    });
    server.tool('godot_render_view', [
        'Render the scene open in the editor from any camera, with no editor gizmos, and return the picture.',
        'Use it to look around a map: stand at a feet position (eye height is added, 1.6 m standing),',
        'face a compass heading (0 = north = -Z, 90 = east = +X) and a pitch in degrees.',
        'Lighting is the open scene\'s own (its WorldEnvironment and lights, or the editor preview sun);',
        'for the exact in-game look use the godot/tests/capture_view.gd script instead.',
    ].join(' '), {
        x: z.number().describe('Feet position x in metres (east +).'),
        y: z.number().describe('Feet position y in metres (up +).'),
        z: z.number().describe('Feet position z in metres (south +).'),
        heading: z.number().optional().describe('Compass heading in degrees, 0 = north (-Z). Default 0.'),
        pitch: z.number().optional().describe('Pitch in degrees, up +. Default 0.'),
        eye_height: z.number().optional().describe('Added to y. Default 1.6 (standing); 1.0 crouched; 0 for a free camera.'),
        fov: z.number().optional().describe('Vertical field of view in degrees. Default 70.'),
        width: z.number().int().optional().describe('Image width in pixels. Default 1280, at most 1920.'),
        height: z.number().int().optional().describe('Image height in pixels. Default 720, at most 1080.'),
        orthographic_size: z.number().optional().describe('If set, an orthographic camera this many metres tall (pitch -90 gives a plan).'),
    }, async (params) => {
        try {
            const res = await bridge.sendRequest('render_view', params);
            return {
                content: [{
                        type: 'image',
                        data: res.image_base64,
                        mimeType: 'image/png'
                    }]
            };
        }
        catch (e) {
            return formatError(e);
        }
    });
}

/**
 *
 * @param {any[]} array
 * @param {(element, index: number, array: any[]) => boolean} filter
 * @returns
 */
export function partition(array, filter) {
    let pass = [],
        fail = [];
    array.forEach((e, idx, arr) => (filter(e, idx, arr) ? pass : fail).push(e));
    return [pass, fail];
}

const reactMount = document.getElementById("MSFS_REACT_MOUNT") as HTMLElement;

export const getRenderTarget = () => reactMount;

export const getRootElement: () => HTMLElement = () => {
	if (reactMount?.parentElement) {
		return reactMount.parentElement;
	}
	throw new Error("Could not find rootElement");
};

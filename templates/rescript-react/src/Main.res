open InfinityMSFS

module Altimeter = {
  @react.component
  let make = () => {
    <div>
     {React.string("hello world")}
    </div>
  }
}

@module("react-dom/client")
external createRoot: Dom.element => 'root = "createRoot"

@send external rootRender: ('root, React.element) => unit = "render"

let mount = (): unit => {
  let target = RenderTarget.getRenderTarget()
  let root = createRoot(target)
  rootRender(
    root,
    <InfinityMSFS.Provider>
      <Altimeter />
    </InfinityMSFS.Provider>,
  )
}